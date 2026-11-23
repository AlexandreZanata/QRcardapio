use chrono::NaiveDate;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::domain::billing::BillingPlan;
use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

#[derive(sqlx::FromRow)]
pub struct Checkout {
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
        "SELECT checkout_id, checkout_status, subscription_id, subscription_status, plan_months, price_centavos
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

pub async fn reserve_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    today: NaiveDate,
    plan: BillingPlan,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    // A creating row has never exposed a link. Allow recovery after the provider's
    // 30-minute checkout lifetime plus a margin for a delayed creation event.
    sqlx::query(
        "UPDATE billing_checkouts SET checkout_status = 'closed'
         WHERE environment = ? AND establishment_id = ? AND checkout_status = 'creating'
           AND checkout_id IS NULL AND created_at <= datetime('now', '-60 minutes')",
    )
    .bind(env)
    .bind(est)
    .execute(&mut *tx)
    .await?;
    let result = sqlx::query(
        "INSERT INTO billing_checkouts (reference, environment, establishment_id, created_at, plan_months, price_centavos)
         SELECT ?, ?, ?, datetime('now'), ?, ? WHERE NOT EXISTS (
            SELECT 1 FROM billing_coverage
            WHERE environment = ? AND establishment_id = ? AND due_date <= ? AND paid_through > ?
         ) ON CONFLICT DO NOTHING",
    ).bind(reference).bind(env).bind(est).bind(plan.months()).bind(plan.price())
        .bind(env).bind(est).bind(today).bind(today)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

pub async fn finish_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    id: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET checkout_id = ?, checkout_status = ?
         WHERE environment = ? AND establishment_id = ? AND reference = ? AND checkout_status = 'creating'",
    ).bind(id).bind(if id.is_some() { "open" } else { "closed" }).bind(env).bind(est).bind(reference)
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

pub struct Payment<'a> {
    pub id: &'a str,
    pub subscription: &'a str,
    pub due_date: NaiveDate,
    pub confirmed_centavos: Option<i64>,
    pub at: &'a str,
    pub priority: i64,
}

pub async fn record_payment(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    payment: Payment<'_>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO billing_payments (environment, payment_id, subscription_id, due_date, confirmed_centavos, event_at, event_priority)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(environment, payment_id) DO UPDATE SET due_date = excluded.due_date,
            confirmed_centavos = excluded.confirmed_centavos, event_at = excluded.event_at, event_priority = excluded.event_priority
         WHERE subscription_id = excluded.subscription_id AND (
            event_at < excluded.event_at OR (event_at = excluded.event_at AND event_priority < excluded.event_priority)
         )",
    ).bind(env).bind(payment.id).bind(payment.subscription).bind(payment.due_date).bind(payment.confirmed_centavos)
        .bind(payment.at).bind(payment.priority).execute(&mut **tx).await?;
    Ok(())
}

pub async fn reconcile_subscription(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    checkout: &str,
    subscription: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_checkouts SET subscription_id = ? WHERE environment = ? AND establishment_id = ?
         AND checkout_status = 'paid' AND checkout_id = ? AND subscription_id IS NULL",
    ).bind(subscription).bind(env).bind(est).bind(checkout).execute(pool).await?;
    Ok(())
}
