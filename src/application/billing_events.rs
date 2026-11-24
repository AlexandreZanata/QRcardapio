//! Applies normalized gateway events (Asaas, Stripe, Inter) to billing state, affiliate
//! commissions and contract activation in one transaction per delivery.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::application::billing::provider_error;
use crate::domain::billing::billing_now;
use crate::domain::payments::{GatewayEvent, PRIORITY_REVERSED, PRIORITY_SETTLED, PaymentProvider};
use crate::error::{AppError, AppResult};
use crate::infrastructure::asaas::Webhook;
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::database::queries::{
    affiliate_ledger, billing as queries, commercial_contracts,
};
use crate::infrastructure::stripe::StripeClient;
use crate::infrastructure::stripe::webhook::{Event as StripeEvent, Parsed as StripeParsed};

/// Stripe webhook: verified, deduplicated by event id, refunds and disputes resolved to the
/// paid invoice through the Stripe API.
pub async fn process_stripe_event(
    pool: &SqlitePool,
    client: &StripeClient,
    event: StripeEvent,
) -> AppResult<()> {
    let env = client.config().environment.as_str();
    let live = env == "production";
    if event.livemode != live {
        return Err(AppError::Validation("error.invalid_billing_event"));
    }
    let key = event
        .dedupe_key()
        .ok_or(AppError::Validation("error.invalid_billing_event"))?;
    let parsed = event.into_parsed().map_err(|reason| {
        tracing::warn!(reason, "invalid Stripe event");
        AppError::Validation("error.invalid_billing_event")
    })?;
    let events = match parsed {
        StripeParsed::Ignore => return Ok(()),
        StripeParsed::Apply(events) => events,
        StripeParsed::Charge {
            payment_intent,
            restore,
            at,
        } => {
            let invoice = client
                .invoice_for_payment_intent(&payment_intent)
                .await
                .map_err(provider_error)?;
            let Some(invoice) = invoice else {
                return Ok(());
            };
            let payment_id = PaymentProvider::Stripe.qualify(&invoice);
            vec![if restore {
                GatewayEvent::PaymentRestored { payment_id, at }
            } else {
                GatewayEvent::PaymentReversed { payment_id, at }
            }]
        }
    };
    apply_events(pool, env, Some(&key), events).await
}

/// Asaas webhook entry point: parses, deduplicates by event id and applies the event.
pub async fn process_webhook(pool: &SqlitePool, env: &str, event: Webhook) -> AppResult<()> {
    let event_id = event.id.clone();
    let Some(change) = event.into_change().map_err(|reason| {
        tracing::warn!(reason, "invalid Asaas event");
        AppError::Validation("error.invalid_billing_event")
    })?
    else {
        return Ok(());
    };
    apply_events(pool, env, Some(&event_id), vec![change]).await
}

/// Applies normalized gateway events in a single `BEGIN IMMEDIATE` transaction: billing
/// state, affiliate commissions and contract activation (outbox) commit together.
/// `dedupe_key` (provider-qualified event id) makes redelivery a no-op; state-based providers
/// (Inter re-fetch) pass `None` because applying the same state twice is idempotent.
pub async fn apply_events(
    pool: &SqlitePool,
    env: &str,
    dedupe_key: Option<&str>,
    events: Vec<GatewayEvent>,
) -> AppResult<()> {
    let mut tx = match dedupe_key {
        Some(key) => match queries::begin_event(pool, env, key).await? {
            Some(tx) => tx,
            None => return Ok(()),
        },
        None => begin_immediate(pool).await?,
    };
    for event in events {
        apply_one(&mut tx, env, dedupe_key, event).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn apply_one(
    tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>,
    env: &str,
    correlation: Option<&str>,
    event: GatewayEvent,
) -> AppResult<()> {
    let today = billing_now().date_naive();
    let now = Utc::now().to_rfc3339();
    match event {
        GatewayEvent::Checkout {
            id,
            reference,
            state,
        } => {
            queries::record_checkout(tx, env, &id, reference.as_deref(), state.as_str()).await?;
        }
        GatewayEvent::Subscription {
            id,
            checkout,
            state,
            at,
        } => {
            if let Some(checkout) = checkout {
                queries::link_subscription(tx, env, &checkout, &id).await?;
                affiliate_ledger::sync_subscription_commissions(tx, env, &id, today, &now).await?;
            }
            queries::record_subscription(tx, env, &id, state.as_str(), &at).await?;
            commercial_contracts::activate_for_subscription(tx, env, &id, correlation).await?;
        }
        GatewayEvent::Payment {
            id,
            checkout,
            subscription_id,
            confirmed_centavos,
            due_date,
            priority,
            at,
        } => {
            let linked_checkout = checkout.is_some();
            if let Some(checkout) = checkout {
                queries::link_subscription(tx, env, &checkout, &subscription_id).await?;
            }
            let payment = queries::Payment {
                id: &id,
                subscription: &subscription_id,
                due_date,
                confirmed_centavos,
                at: &at,
                priority,
            };
            queries::record_payment(tx, env, payment).await?;
            if linked_checkout {
                affiliate_ledger::sync_subscription_commissions(
                    tx,
                    env,
                    &subscription_id,
                    today,
                    &now,
                )
                .await?;
            } else {
                affiliate_ledger::sync_payment_commissions(tx, env, &id, today, &now).await?;
            }
            commercial_contracts::activate_for_subscription(tx, env, &subscription_id, correlation)
                .await?;
        }
        GatewayEvent::PaymentReversed { payment_id, at } => {
            apply_payment_change(tx, env, &payment_id, &at, false, today, &now).await?;
        }
        GatewayEvent::PaymentRestored { payment_id, at } => {
            if let Some(subscription) =
                apply_payment_change(tx, env, &payment_id, &at, true, today, &now).await?
            {
                commercial_contracts::activate_for_subscription(
                    tx,
                    env,
                    &subscription,
                    correlation,
                )
                .await?;
            }
        }
    }
    Ok(())
}

/// Reverses or restores a recorded payment. Returns its subscription when something changed.
async fn apply_payment_change(
    tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>,
    env: &str,
    payment_id: &str,
    at: &str,
    restore: bool,
    today: chrono::NaiveDate,
    now: &str,
) -> AppResult<Option<String>> {
    let Some(found) = queries::payment_by_id(tx, env, payment_id).await? else {
        // Settlement not seen yet; reconciliation recomputes from the provider later.
        tracing::info!("reversal for an unknown payment ignored");
        return Ok(None);
    };
    let (confirmed, priority) = match (restore, found.settled_centavos) {
        (true, Some(settled)) => (Some(settled), PRIORITY_SETTLED),
        (true, None) => return Ok(None),
        (false, _) => (None, PRIORITY_REVERSED),
    };
    let payment = queries::Payment {
        id: &found.payment_id,
        subscription: &found.subscription_id,
        due_date: found.due_date,
        confirmed_centavos: confirmed,
        at,
        priority,
    };
    queries::record_payment(tx, env, payment).await?;
    affiliate_ledger::sync_payment_commissions(tx, env, &found.payment_id, today, now).await?;
    Ok(Some(found.subscription_id))
}
