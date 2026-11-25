use chrono::NaiveDate;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::commercial_contracts;

pub use super::billing_payments::*;
use crate::domain::billing::BillingPlan;
use crate::domain::commercial::Quote;
use crate::domain::payments::{PaymentMethod, PaymentProvider};
use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

#[derive(sqlx::FromRow)]
pub struct Checkout {
    pub reference: String,
    pub provider: String,
    pub payment_method: String,
    pub checkout_url: Option<String>,
    pub checkout_id: Option<String>,
    pub checkout_status: String,
    pub subscription_id: Option<String>,
    pub subscription_status: String,
    pub plan_months: i64,
    pub price_centavos: i64,
}

pub async fn current_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
) -> AppResult<Option<Checkout>> {
    Ok(sqlx::query_as(
        "SELECT reference, provider, payment_method, checkout_url, checkout_id, checkout_status,
                subscription_id, subscription_status, plan_months, price_centavos
         FROM billing_checkouts WHERE environment = ? AND establishment_id = ?
           AND checkout_status <> 'closed' AND subscription_status <> 'deleted'
           AND (checkout_status <> 'creating' OR created_at > datetime('now', '-60 minutes'))",
    )
    .bind(env)
    .bind(est)
    .fetch_optional(pool)
    .await?)
}

pub async fn paid_through(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    today: NaiveDate,
) -> AppResult<Option<NaiveDate>> {
    Ok(paid_coverage(pool, env, est, today)
        .await?
        .map(|coverage| coverage.paid_through))
}

#[derive(sqlx::FromRow)]
pub struct Coverage {
    pub paid_through: NaiveDate,
    pub plan_months: i64,
    pub price_centavos: i64,
}

pub async fn paid_coverage(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    today: NaiveDate,
) -> AppResult<Option<Coverage>> {
    Ok(sqlx::query_as(
        "SELECT paid_through, plan_months, price_centavos FROM billing_coverage
         WHERE environment = ? AND establishment_id = ? AND due_date <= ? AND paid_through > ?
         ORDER BY paid_through DESC, due_date DESC LIMIT 1",
    )
    .bind(env)
    .bind(est)
    .bind(today)
    .bind(today)
    .fetch_optional(pool)
    .await?)
}

/// Reserves a checkout for the entry offer (QR Menu) on a cycle. Kept for the legacy
/// "Plano Essencial" flow and its tests; new code calls [`reserve_contract_checkout`].
pub async fn reserve_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    today: NaiveDate,
    plan: BillingPlan,
) -> AppResult<bool> {
    let quote = crate::domain::commercial::catalog().entry_quote(plan);
    reserve_contract_checkout(
        pool,
        env,
        est,
        reference,
        today,
        &quote,
        CheckoutTarget::ASAAS_CARD,
    )
    .await
}

/// Provider and method of a new checkout. Providers without hosted checkout (Inter) open the
/// local subscription immediately under synthetic, provider-qualified identifiers.
#[derive(Debug, Clone, Copy)]
pub struct CheckoutTarget {
    pub provider: PaymentProvider,
    pub method: PaymentMethod,
}

impl CheckoutTarget {
    pub const ASAAS_CARD: Self = Self {
        provider: PaymentProvider::Asaas,
        method: PaymentMethod::Card,
    };
}

/// Reserves a checkout and stores its commercial contract snapshot atomically. The gateway
/// amount (`billing_checkouts.price_centavos`) is the quote total computed by the backend.
pub async fn reserve_contract_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    today: NaiveDate,
    quote: &Quote,
    target: CheckoutTarget,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let reserved = reserve_contract_checkout_in_transaction(
        &mut tx, env, est, reference, today, quote, target,
    )
    .await?;
    tx.commit().await?;
    Ok(reserved)
}

pub(crate) async fn reserve_contract_checkout_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    est: &str,
    reference: &str,
    today: NaiveDate,
    quote: &Quote,
    target: CheckoutTarget,
) -> AppResult<bool> {
    // A creating row has never exposed a link. Allow recovery after the provider's
    // 30-minute checkout lifetime plus a margin for a delayed creation event.
    sqlx::query(
        "UPDATE billing_checkouts SET checkout_status = 'closed'
         WHERE environment = ? AND establishment_id = ? AND checkout_status = 'creating'
           AND checkout_id IS NULL AND created_at <= datetime('now', '-60 minutes')",
    )
    .bind(env)
    .bind(est)
    .execute(&mut **tx)
    .await?;
    // Inter has no hosted checkout or provider subscription: our reference is both.
    let local_id =
        (target.provider == PaymentProvider::Inter).then(|| target.provider.qualify(reference));
    let (checkout_status, subscription_status) = match local_id {
        Some(_) => ("open", "active"),
        None => ("creating", "pending"),
    };
    let result = sqlx::query(
        "INSERT INTO billing_checkouts (reference, environment, establishment_id, created_at, plan_months,
            price_centavos, provider, payment_method, checkout_id, checkout_status, subscription_id,
            subscription_status)
         SELECT ?, ?, ?, datetime('now'), ?, ?, ?, ?, ?, ?, ?, ? WHERE NOT EXISTS (
            SELECT 1 FROM billing_coverage
            WHERE environment = ? AND establishment_id = ? AND due_date <= ? AND paid_through > ?
         ) ON CONFLICT DO NOTHING",
    ).bind(reference).bind(env).bind(est).bind(quote.cycle.months()).bind(quote.total_centavos)
        .bind(target.provider.as_str()).bind(target.method.as_str()).bind(local_id.as_deref())
        .bind(checkout_status).bind(local_id.as_deref()).bind(subscription_status)
        .bind(env).bind(est).bind(today).bind(today)
        .execute(&mut **tx).await?;
    let reserved = result.rows_affected() == 1;
    if reserved {
        commercial_contracts::insert_contract(tx, reference, env, est, reference, quote).await?;
    }
    Ok(reserved)
}

pub async fn finish_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    id: Option<&str>,
) -> AppResult<()> {
    finish_hosted_checkout(pool, env, est, reference, id, None).await
}

/// Stores the hosted checkout id (and URL when the provider does not derive it from the id).
pub async fn finish_hosted_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    id: Option<&str>,
    url: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET checkout_id = ?, checkout_status = ?, checkout_url = ?
         WHERE environment = ? AND establishment_id = ? AND reference = ? AND checkout_status = 'creating'",
    ).bind(id).bind(if id.is_some() { "open" } else { "closed" }).bind(url).bind(env).bind(est).bind(reference)
        .execute(pool).await?;
    Ok(())
}

pub async fn cancel_subscription(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    id: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET subscription_status = 'deleted'
         WHERE environment = ? AND establishment_id = ? AND subscription_id = ?",
    )
    .bind(env)
    .bind(est)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn begin_event(
    pool: &SqlitePool,
    env: &str,
    event_id: &str,
) -> AppResult<Option<Transaction<'static, Sqlite>>> {
    let seen: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM billing_webhook_events WHERE environment = ? AND event_id = ?)",
    )
    .bind(env)
    .bind(event_id)
    .fetch_one(pool)
    .await?;
    if seen {
        return Ok(None);
    }
    // Concurrent first deliveries still contend on the unique key inside the transaction.
    let mut tx = begin_immediate(pool).await?;
    let inserted = sqlx::query(
        "INSERT INTO billing_webhook_events (environment, event_id) VALUES (?, ?) ON CONFLICT DO NOTHING",
    ).bind(env).bind(event_id).execute(&mut *tx).await?;
    if inserted.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(None);
    }
    Ok(Some(tx))
}

pub async fn record_checkout(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    id: &str,
    reference: Option<&str>,
    status: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET checkout_id = ?, checkout_status = ?
         WHERE environment = ? AND (checkout_id = ? OR (checkout_id IS NULL AND reference = ?))
           AND (checkout_status = 'creating' OR (checkout_status = 'open' AND ? <> 'open'))",
    )
    .bind(id)
    .bind(status)
    .bind(env)
    .bind(id)
    .bind(reference)
    .bind(status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn link_subscription(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    checkout: &str,
    id: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET subscription_id = ?, checkout_status = 'paid'
         WHERE environment = ? AND checkout_id = ? AND checkout_status IN ('open', 'paid')
           AND subscription_id IS NULL",
    )
    .bind(id)
    .bind(env)
    .bind(checkout)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn record_subscription(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    id: &str,
    status: &str,
    at: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET subscription_status = ?, subscription_event_at = ?
         WHERE environment = ? AND subscription_id = ? AND subscription_status <> 'deleted'
           AND (subscription_event_at IS NULL OR subscription_event_at < ?
                OR (subscription_event_at = ? AND ? IN ('deleted', 'inactive')))",
    )
    .bind(status)
    .bind(at)
    .bind(env)
    .bind(id)
    .bind(at)
    .bind(at)
    .bind(status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn reconcile_subscription(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    checkout: &str,
    subscription: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let res = sqlx::query(
        "UPDATE billing_checkouts SET subscription_id = ? WHERE environment = ? AND establishment_id = ?
         AND checkout_status = 'paid' AND checkout_id = ? AND subscription_id IS NULL",
    )
    .bind(subscription)
    .bind(env)
    .bind(est)
    .bind(checkout)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 1 {
        commercial_contracts::activate_for_subscription(&mut tx, env, subscription, None).await?;
        let today = crate::domain::billing::billing_now().date_naive();
        let now = chrono::Utc::now().to_rfc3339();
        super::affiliate_ledger::sync_subscription_commissions(
            &mut tx,
            env,
            subscription,
            today,
            &now,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
