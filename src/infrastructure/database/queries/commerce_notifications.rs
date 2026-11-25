use chrono::{DateTime, Utc};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use super::domain_events::StoredEvent;
use crate::error::AppResult;
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::web_push::{DeliveryResult, PushSubscription};

pub(crate) async fn register(
    pool: &SqlitePool,
    establishment_id: &str,
    account_id: &str,
    subscription: &PushSubscription,
    now: DateTime<Utc>,
) -> AppResult<Option<String>> {
    let _turn = commerce_writes::enter(pool).await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO commerce_push_subscriptions
            (id, establishment_id, account_id, endpoint, p256dh, auth_secret, registered_at)
         SELECT ?1, establishment_id, account_id, ?4, ?5, ?6, ?7
         FROM establishment_members WHERE establishment_id = ?2 AND account_id = ?3
         ON CONFLICT(endpoint) DO UPDATE SET p256dh = excluded.p256dh,
            auth_secret = excluded.auth_secret
         WHERE establishment_id = ?2 AND account_id = ?3 RETURNING id",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(establishment_id)
    .bind(account_id)
    .bind(subscription.endpoint())
    .bind(subscription.public_key())
    .bind(subscription.auth_secret())
    .bind(now.timestamp())
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn revoke(
    pool: &SqlitePool,
    establishment_id: &str,
    account_id: &str,
    id: &str,
) -> AppResult<bool> {
    let _turn = commerce_writes::enter(pool).await?;
    Ok(sqlx::query(
        "DELETE FROM commerce_push_subscriptions
         WHERE establishment_id = ? AND account_id = ? AND id = ?",
    )
    .bind(establishment_id)
    .bind(account_id)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

pub(crate) async fn enqueue(
    tx: &mut Transaction<'_, Sqlite>,
    event: &StoredEvent,
    now: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO commerce_push_deliveries
            (establishment_id, event_id, subscription_id, order_id, available_at, expires_at)
         SELECT o.establishment_id, ?1, s.id, o.id, ?2,
                unixepoch(o.acceptance_deadline_at) + 1
         FROM orders o JOIN commerce_push_subscriptions s USING (establishment_id)
         WHERE o.establishment_id = ?3 AND o.id = ?4 AND o.state = 'placed'
           AND o.acceptance_deadline_at > ?5 AND s.registered_at <= unixepoch(?6)
         ON CONFLICT(establishment_id, event_id, subscription_id) DO NOTHING",
    )
    .bind(&event.event_id)
    .bind(now.timestamp())
    .bind(&event.establishment_id)
    .bind(&event.aggregate_id)
    .bind(now)
    .bind(&event.occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub(crate) struct PushDelivery {
    pub establishment_id: String,
    pub event_id: String,
    pub subscription_id: String,
    pub account_id: String,
    pub endpoint: String,
    pub p256dh: String,
    pub auth_secret: String,
    pub order_number: i64,
    pub locale: String,
    pub attempts: u32,
    pub remaining_seconds: i64,
}

const MAX_ATTEMPTS: i64 = 8;
pub(crate) const MAX_IN_FLIGHT: usize = 8;

/// A short database lease prevents duplicate claims. Network I/O follows the commit.
pub(crate) async fn claim(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    token: &str,
) -> AppResult<Vec<PushDelivery>> {
    let now_seconds = now.timestamp();
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let deliveries = sqlx::query_as::<_, PushDelivery>(
        "SELECT d.establishment_id, d.event_id, d.subscription_id, s.account_id,
                s.endpoint, s.p256dh, s.auth_secret, o.order_number,
                a.preferred_locale AS locale, d.attempts + 1 AS attempts,
                max(0, unixepoch(o.acceptance_deadline_at) - ?1) AS remaining_seconds
         FROM commerce_push_deliveries d
         JOIN commerce_push_subscriptions s
           ON s.establishment_id = d.establishment_id AND s.id = d.subscription_id
         JOIN accounts a ON a.id = s.account_id
         JOIN orders o ON o.establishment_id = d.establishment_id AND o.id = d.order_id
         WHERE d.finished_at IS NULL AND d.available_at <= ?1 AND d.attempts < ?2
           AND o.state = 'placed' AND o.acceptance_deadline_at > ?3
         ORDER BY d.available_at, d.event_id, d.subscription_id LIMIT ?4",
    )
    .bind(now_seconds)
    .bind(MAX_ATTEMPTS)
    .bind(now)
    .bind(MAX_IN_FLIGHT as i64)
    .fetch_all(&mut *tx)
    .await?;
    for delivery in &deliveries {
        sqlx::query(
            "UPDATE commerce_push_deliveries
             SET available_at = ?, lease_token = ?, attempts = ?
             WHERE establishment_id = ? AND event_id = ? AND subscription_id = ?",
        )
        .bind(now_seconds + 30)
        .bind(token)
        .bind(delivery.attempts)
        .bind(&delivery.establishment_id)
        .bind(&delivery.event_id)
        .bind(&delivery.subscription_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(deliveries)
}

pub(crate) async fn record_results(
    pool: &SqlitePool,
    results: &[(&PushDelivery, DeliveryResult)],
    token: &str,
    now_seconds: i64,
) -> AppResult<()> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    for &(delivery, result) in results {
        record_result_in(&mut tx, delivery, token, result, now_seconds).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn record_result_in(
    tx: &mut Transaction<'_, Sqlite>,
    delivery: &PushDelivery,
    token: &str,
    result: DeliveryResult,
    now_seconds: i64,
) -> AppResult<()> {
    let retry_at = match result {
        DeliveryResult::Retry { after_seconds } if i64::from(delivery.attempts) < MAX_ATTEMPTS => {
            let backoff = (1 << delivery.attempts).min(60);
            Some(now_seconds + after_seconds.unwrap_or(0).clamp(backoff, 3600))
        }
        _ => None,
    };
    let outcome = retry_at.is_none().then_some(match result {
        DeliveryResult::Delivered => "delivered",
        DeliveryResult::Rejected => "rejected",
        DeliveryResult::Gone => "gone",
        DeliveryResult::Retry { .. } => "exhausted",
    });
    let changed = sqlx::query(
        "UPDATE commerce_push_deliveries SET
            finished_at = ?1, available_at = coalesce(?2, available_at),
            outcome = ?3, lease_token = NULL
         WHERE establishment_id = ?4 AND event_id = ?5 AND subscription_id = ?6
           AND lease_token = ?7",
    )
    .bind(retry_at.is_none().then_some(now_seconds))
    .bind(retry_at)
    .bind(outcome)
    .bind(&delivery.establishment_id)
    .bind(&delivery.event_id)
    .bind(&delivery.subscription_id)
    .bind(token)
    .execute(&mut **tx)
    .await?;
    if changed.rows_affected() != 0 && result == DeliveryResult::Gone {
        sqlx::query(
            "DELETE FROM commerce_push_subscriptions
             WHERE establishment_id = ? AND id = ? AND account_id = ?
               AND p256dh = ? AND auth_secret = ?",
        )
        .bind(&delivery.establishment_id)
        .bind(&delivery.subscription_id)
        .bind(&delivery.account_id)
        .bind(&delivery.p256dh)
        .bind(&delivery.auth_secret)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Platform housekeeping retains outcome records for seven days past the order deadline.
pub(crate) async fn prune(pool: &SqlitePool, older_than: i64) -> AppResult<()> {
    let _turn = commerce_writes::enter(pool).await?;
    sqlx::query(
        "DELETE FROM commerce_push_deliveries WHERE (establishment_id, event_id, subscription_id) IN (
            SELECT establishment_id, event_id, subscription_id FROM commerce_push_deliveries
            WHERE expires_at <= ? ORDER BY expires_at LIMIT 1000
         )",
    )
    .bind(older_than)
    .execute(pool)
    .await?;
    Ok(())
}

/// Finalizes expired notifications and sends interrupted during the last permitted lease.
pub(crate) async fn finish_expired(pool: &SqlitePool, now: i64) -> AppResult<()> {
    let _turn = commerce_writes::enter(pool).await?;
    sqlx::query(
        "UPDATE commerce_push_deliveries SET finished_at = ?1, lease_token = NULL,
            outcome = CASE WHEN attempts >= ?2 THEN 'exhausted' ELSE 'expired' END
         WHERE (establishment_id, event_id, subscription_id) IN (
            SELECT establishment_id, event_id, subscription_id FROM commerce_push_deliveries
            WHERE finished_at IS NULL AND (
                (expires_at <= ?1 AND (lease_token IS NULL OR available_at <= ?1))
                OR (attempts >= ?2 AND available_at <= ?1)
            ) ORDER BY available_at LIMIT 1000
         )",
    )
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .execute(pool)
    .await?;
    Ok(())
}
