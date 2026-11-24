use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::billing::{BillingPlan, billing_now};
use crate::error::{AppError, AppResult};
use crate::infrastructure::asaas::{AsaasClient, Change, GatewayError, Webhook};
use crate::infrastructure::database::queries::{affiliate_ledger, billing as queries};

pub async fn publication_allowed(
    pool: &SqlitePool,
    client: Option<&AsaasClient>,
    est: &str,
) -> AppResult<bool> {
    let Some(client) = client else {
        return Ok(true);
    };
    let env = client.config().environment.as_str();
    Ok(
        queries::paid_through(pool, env, est, billing_now().date_naive())
            .await?
            .is_some(),
    )
}

pub async fn start_checkout(
    pool: &SqlitePool,
    client: &AsaasClient,
    public_base: &str,
    establishment: &str,
    plan: BillingPlan,
) -> AppResult<()> {
    let env = client.config().environment.as_str();
    let reference = Uuid::now_v7();
    let reference_text = reference.to_string();
    if queries::reserve_checkout(
        pool,
        env,
        establishment,
        &reference_text,
        billing_now().date_naive(),
        plan,
    )
    .await?
    {
        match client.create_checkout(public_base, reference, plan).await {
            Ok(id) => {
                queries::finish_checkout(
                    pool,
                    env,
                    establishment,
                    &reference_text,
                    Some(&id.to_string()),
                )
                .await?
            }
            Err(error) => {
                if error.rejected_checkout() {
                    queries::finish_checkout(pool, env, establishment, &reference_text, None)
                        .await?;
                }
                return Err(provider_error(error));
            }
        }
    }
    Ok(())
}

pub async fn reconcile_checkout(
    pool: &SqlitePool,
    client: &AsaasClient,
    establishment: &str,
) -> AppResult<bool> {
    let env = client.config().environment.as_str();
    let id = queries::current_checkout(pool, env, establishment)
        .await?
        .filter(|row| row.checkout_status == "paid" && row.subscription_id.is_none())
        .and_then(|row| row.checkout_id)
        .and_then(|id| Uuid::parse_str(&id).ok());
    let Some(id) = id else { return Ok(false) };
    let Some(subscription) = client
        .checkout_subscription(id)
        .await
        .map_err(provider_error)?
    else {
        return Ok(false);
    };
    queries::reconcile_subscription(pool, env, establishment, &id.to_string(), &subscription)
        .await?;
    Ok(true)
}

pub async fn cancel_subscription(
    pool: &SqlitePool,
    client: &AsaasClient,
    establishment: &str,
    expected_id: &str,
) -> AppResult<()> {
    let env = client.config().environment.as_str();
    let subscription_id = queries::current_checkout(pool, env, establishment)
        .await?
        .and_then(|row| row.subscription_id);
    if subscription_id.as_deref() != Some(expected_id) {
        return Err(AppError::Conflict("error.billing_subscription_changed"));
    }
    client
        .delete_subscription(expected_id)
        .await
        .map_err(provider_error)?;
    queries::cancel_subscription(pool, env, establishment, expected_id).await?;
    Ok(())
}

pub async fn process_webhook(pool: &SqlitePool, env: &str, event: Webhook) -> AppResult<()> {
    let event_id = event.id.clone();
    let Some(change) = event.into_change().map_err(|reason| {
        tracing::warn!(reason, "invalid Asaas event");
        AppError::Validation("error.invalid_billing_event")
    })?
    else {
        return Ok(());
    };
    let Some(mut tx) = queries::begin_event(pool, env, &event_id).await? else {
        return Ok(());
    };
    match change {
        Change::Checkout {
            id,
            reference,
            status,
        } => {
            let reference = reference.map(|value| value.to_string());
            queries::record_checkout(&mut tx, env, &id.to_string(), reference.as_deref(), status)
                .await?;
        }
        Change::Subscription {
            id,
            checkout,
            status,
            at,
        } => {
            if let Some(checkout) = checkout {
                queries::link_subscription(&mut tx, env, &checkout.to_string(), &id).await?;
                let today = billing_now().date_naive();
                let now = Utc::now().to_rfc3339();
                affiliate_ledger::sync_subscription_commissions(&mut tx, env, &id, today, &now)
                    .await?;
            }
            queries::record_subscription(&mut tx, env, &id, status, &at).await?;
        }
        Change::Payment {
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
                queries::link_subscription(&mut tx, env, &checkout.to_string(), &subscription_id)
                    .await?;
            }
            queries::record_payment(
                &mut tx,
                env,
                queries::Payment {
                    id: &id,
                    subscription: &subscription_id,
                    due_date,
                    confirmed_centavos,
                    at: &at,
                    priority,
                },
            )
            .await?;
            let today = billing_now().date_naive();
            let now = Utc::now().to_rfc3339();
            if linked_checkout {
                affiliate_ledger::sync_subscription_commissions(
                    &mut tx,
                    env,
                    &subscription_id,
                    today,
                    &now,
                )
                .await?;
            } else {
                affiliate_ledger::sync_payment_commissions(&mut tx, env, &id, today, &now).await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

fn provider_error(error: GatewayError) -> AppError {
    tracing::warn!(%error, "Asaas operation failed");
    AppError::Unavailable("error.billing_unavailable")
}
