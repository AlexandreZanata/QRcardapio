use chrono::NaiveDate;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

#[derive(sqlx::FromRow)]
pub struct Checkout {
    pub checkout_id: Option<String>,
    pub checkout_status: String,
    pub subscription_id: Option<String>,
    pub subscription_status: String,
}

pub async fn current_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
) -> AppResult<Option<Checkout>> {
    Ok(sqlx::query_as(
        "SELECT checkout_id, checkout_status, subscription_id, subscription_status
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
    Ok(sqlx::query_scalar(
        "SELECT MAX(p.paid_through) FROM billing_payments p
         INNER JOIN billing_checkouts c ON c.environment = p.environment AND c.subscription_id = p.subscription_id
         WHERE c.environment = ? AND c.establishment_id = ? AND p.due_date <= ? AND p.paid_through > ?",
    ).bind(env).bind(est).bind(today).bind(today).fetch_one(pool).await?)
}

pub async fn reserve_checkout(
    pool: &SqlitePool,
    env: &str,
    est: &str,
    reference: &str,
    today: NaiveDate,
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
        "INSERT INTO billing_checkouts (reference, environment, establishment_id, created_at)
         SELECT ?, ?, ?, datetime('now') WHERE NOT EXISTS (
            SELECT 1 FROM billing_payments p
            INNER JOIN billing_checkouts c ON c.environment = p.environment AND c.subscription_id = p.subscription_id
            WHERE c.environment = ? AND c.establishment_id = ? AND p.due_date <= ? AND p.paid_through > ?
         ) ON CONFLICT DO NOTHING",
    ).bind(reference).bind(env).bind(est).bind(env).bind(est).bind(today).bind(today)
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
    pub paid_through: Option<NaiveDate>,
    pub at: &'a str,
    pub priority: i64,
}

pub async fn record_payment(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    payment: Payment<'_>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO billing_payments (environment, payment_id, subscription_id, due_date, paid_through, event_at, event_priority)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(environment, payment_id) DO UPDATE SET due_date = excluded.due_date,
            paid_through = excluded.paid_through, event_at = excluded.event_at, event_priority = excluded.event_priority
         WHERE subscription_id = excluded.subscription_id AND (
            event_at < excluded.event_at OR (event_at = excluded.event_at AND event_priority < excluded.event_priority)
         )",
    ).bind(env).bind(payment.id).bind(payment.subscription).bind(payment.due_date).bind(payment.paid_through)
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
