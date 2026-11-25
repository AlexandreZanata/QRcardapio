//! SaaS subscription use cases routed to the provider of each payment method.

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::billing_inter;
use crate::application::payments::BillingGateways;
use crate::domain::billing::billing_now;
use crate::domain::commercial::Quote;
use crate::domain::payments::{PaymentMethod, PaymentProvider};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::billing as queries;
use crate::infrastructure::gateway::GatewayError;

fn unavailable() -> AppError {
    AppError::NotFound("error.not_found")
}

/// Starts a subscription with the provider routed for `method`. Card methods open a hosted
/// recurring checkout (Asaas or Stripe); Pix/boleto issues the first Inter charge. The
/// commercial contract snapshot is stored in the same transaction as the reservation.
pub async fn start_checkout(
    pool: &SqlitePool,
    gateways: &BillingGateways,
    public_base: &str,
    establishment: &str,
    method: PaymentMethod,
    quote: &Quote,
) -> AppResult<()> {
    let provider = gateways
        .provider_for(method)
        .ok_or(AppError::Validation("error.payment_method_unavailable"))?;
    match provider {
        PaymentProvider::Inter => {
            let client = gateways.inter.as_ref().ok_or_else(unavailable)?;
            billing_inter::start(pool, client, establishment, quote).await
        }
        PaymentProvider::Asaas | PaymentProvider::Stripe => {
            start_hosted_checkout(pool, gateways, provider, public_base, establishment, quote).await
        }
    }
}

async fn start_hosted_checkout(
    pool: &SqlitePool,
    gateways: &BillingGateways,
    provider: PaymentProvider,
    public_base: &str,
    establishment: &str,
    quote: &Quote,
) -> AppResult<()> {
    let env = gateways.environment().ok_or_else(unavailable)?;
    let reference = Uuid::now_v7();
    let reference_text = reference.to_string();
    let target = queries::CheckoutTarget {
        provider,
        method: PaymentMethod::Card,
    };
    let today = billing_now().date_naive();
    if !queries::reserve_contract_checkout(
        pool,
        env,
        establishment,
        &reference_text,
        today,
        quote,
        target,
    )
    .await?
    {
        return Ok(());
    }
    let created = match provider {
        PaymentProvider::Asaas => {
            let client = gateways.asaas.as_ref().ok_or_else(unavailable)?;
            client
                .create_checkout(public_base, reference, quote)
                .await
                .map(|id| (id.to_string(), None))
        }
        PaymentProvider::Stripe => {
            let client = gateways.stripe.as_ref().ok_or_else(unavailable)?;
            client
                .create_checkout(public_base, reference, establishment, quote)
                .await
                .map(|session| (provider.qualify(&session.id), Some(session.url)))
        }
        PaymentProvider::Inter => return Err(unavailable()),
    };
    match created {
        Ok((id, url)) => {
            queries::finish_hosted_checkout(
                pool,
                env,
                establishment,
                &reference_text,
                Some(&id),
                url.as_deref(),
            )
            .await
        }
        Err(error) => {
            if error.rejected_checkout() {
                queries::finish_checkout(pool, env, establishment, &reference_text, None).await?;
            }
            Err(provider_error(error))
        }
    }
}

/// Links a paid checkout whose subscription event was missed (or polls Inter charges).
pub async fn reconcile_checkout(
    pool: &SqlitePool,
    gateways: &BillingGateways,
    establishment: &str,
) -> AppResult<bool> {
    let env = gateways.environment().ok_or_else(unavailable)?;
    let Some(row) = queries::current_checkout(pool, env, establishment).await? else {
        return Ok(false);
    };
    let provider = PaymentProvider::parse(&row.provider).unwrap_or(PaymentProvider::Asaas);
    if provider == PaymentProvider::Inter {
        let client = gateways.inter.as_ref().ok_or_else(unavailable)?;
        billing_inter::run_cycle(pool, client).await?;
        return Ok(true);
    }
    if row.checkout_status != "paid" || row.subscription_id.is_some() {
        return Ok(false);
    }
    let Some(checkout_id) = row.checkout_id else {
        return Ok(false);
    };
    let subscription = match provider {
        PaymentProvider::Asaas => {
            let Ok(id) = Uuid::parse_str(&checkout_id) else {
                return Ok(false);
            };
            let client = gateways.asaas.as_ref().ok_or_else(unavailable)?;
            client.checkout_subscription(id).await
        }
        PaymentProvider::Stripe => {
            let client = gateways.stripe.as_ref().ok_or_else(unavailable)?;
            let Some(session) = provider.unqualify(&checkout_id) else {
                return Ok(false);
            };
            client
                .session_subscription(session)
                .await
                .map(|sub| sub.map(|id| provider.qualify(&id)))
        }
        PaymentProvider::Inter => return Ok(false),
    }
    .map_err(provider_error)?;
    let Some(subscription) = subscription else {
        return Ok(false);
    };
    queries::reconcile_subscription(pool, env, establishment, &checkout_id, &subscription).await?;
    Ok(true)
}

/// Cancels renewal with the subscription's own provider. Paid periods stay covered.
pub async fn cancel_subscription(
    pool: &SqlitePool,
    gateways: &BillingGateways,
    establishment: &str,
    expected_id: &str,
) -> AppResult<()> {
    let env = gateways.environment().ok_or_else(unavailable)?;
    let current = queries::current_checkout(pool, env, establishment).await?;
    let Some(row) = current.filter(|row| row.subscription_id.as_deref() == Some(expected_id))
    else {
        return Err(AppError::Conflict("error.billing_subscription_changed"));
    };
    match PaymentProvider::parse(&row.provider).unwrap_or(PaymentProvider::Asaas) {
        PaymentProvider::Asaas => {
            let client = gateways.asaas.as_ref().ok_or_else(unavailable)?;
            client
                .delete_subscription(expected_id)
                .await
                .map_err(provider_error)?;
        }
        PaymentProvider::Stripe => {
            let client = gateways.stripe.as_ref().ok_or_else(unavailable)?;
            let id = PaymentProvider::Stripe
                .unqualify(expected_id)
                .ok_or(AppError::Conflict("error.billing_subscription_changed"))?;
            client
                .cancel_at_period_end(id)
                .await
                .map_err(provider_error)?;
        }
        PaymentProvider::Inter => {
            let client = gateways.inter.as_ref().ok_or_else(unavailable)?;
            billing_inter::cancel(pool, client, &row.reference).await?;
        }
    }
    queries::cancel_subscription(pool, env, establishment, expected_id).await?;
    Ok(())
}

pub(crate) fn provider_error(error: GatewayError) -> AppError {
    tracing::warn!(%error, "payment provider operation failed");
    AppError::Unavailable("error.billing_unavailable")
}

pub use super::billing_events::{apply_events, process_stripe_event, process_webhook};
