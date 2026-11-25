//! Gateway payments: order-safe recording and lookup for reversals.

use chrono::NaiveDate;
use sqlx::{Sqlite, Transaction};

use crate::error::AppResult;

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
    // Order-independent fact: the last settled amount (restored when a dispute is won).
    sqlx::query(
        "UPDATE billing_payments SET settled_centavos = COALESCE(?, settled_centavos)
         WHERE environment = ? AND payment_id = ? AND subscription_id = ?",
    )
    .bind(payment.confirmed_centavos)
    .bind(env)
    .bind(payment.id)
    .bind(payment.subscription)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub struct ChargedPayment {
    pub payment_id: String,
    pub subscription_id: String,
    pub due_date: NaiveDate,
    pub settled_centavos: Option<i64>,
}

/// A recorded payment (refunds and disputes reference it after the provider resolves it).
pub async fn payment_by_id(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    payment_id: &str,
) -> AppResult<Option<ChargedPayment>> {
    Ok(sqlx::query_as(
        "SELECT payment_id, subscription_id, due_date, settled_centavos FROM billing_payments
         WHERE environment = ? AND payment_id = ?",
    )
    .bind(env)
    .bind(payment_id)
    .fetch_optional(&mut **tx)
    .await?)
}
