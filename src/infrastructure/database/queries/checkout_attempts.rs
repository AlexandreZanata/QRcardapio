use chrono::{DateTime, Duration, Utc};
use sqlx::{SqliteConnection, SqlitePool};

use crate::domain::commerce::order::Order;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;

const RETENTION: Duration = Duration::days(7);

pub(crate) async fn find_live(
    conn: &mut SqliteConnection,
    establishment_id: &str,
    key_hash: &[u8],
    now: DateTime<Utc>,
) -> AppResult<Option<(Vec<u8>, String)>> {
    Ok(sqlx::query_as(
        "SELECT a.request_hash, o.public_id FROM checkout_attempts a
         JOIN orders o ON o.id = a.order_id AND o.establishment_id = a.establishment_id
         WHERE a.idempotency_key_hash = ? AND a.establishment_id = ? AND a.expires_at > ?",
    )
    .bind(key_hash)
    .bind(establishment_id)
    .bind(now.timestamp())
    .fetch_optional(conn)
    .await?)
}

pub(crate) async fn remember(
    conn: &mut SqliteConnection,
    order: &Order,
    key_hash: &[u8],
    request_hash: &[u8],
) -> AppResult<()> {
    let created_at = order.placed_at.timestamp();
    let expires_at = created_at + RETENTION.num_seconds();
    let result = sqlx::query(
        "INSERT INTO checkout_attempts (establishment_id, idempotency_key_hash, request_hash,
                                        order_id, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (establishment_id, idempotency_key_hash) DO UPDATE
         SET request_hash = excluded.request_hash, order_id = excluded.order_id,
             created_at = excluded.created_at, expires_at = excluded.expires_at
         WHERE checkout_attempts.expires_at <= excluded.created_at
           AND checkout_attempts.establishment_id = excluded.establishment_id",
    )
    .bind(&order.establishment_id)
    .bind(key_hash)
    .bind(request_hash)
    .bind(&order.id)
    .bind(created_at)
    .bind(expires_at)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("commerce.idempotency_mismatch"));
    }
    Ok(())
}

/// Bounded global maintenance for the worker. Checkout observes expiration independently.
pub async fn prune_expired(pool: &SqlitePool, now: DateTime<Utc>, limit: u32) -> AppResult<u64> {
    let _turn = commerce_writes::enter(pool).await?;
    Ok(sqlx::query(
        "DELETE FROM checkout_attempts WHERE (establishment_id, idempotency_key_hash) IN (
             SELECT establishment_id, idempotency_key_hash FROM checkout_attempts WHERE expires_at <= ?
             ORDER BY expires_at, establishment_id, idempotency_key_hash LIMIT ?
         )",
    )
    .bind(now.timestamp())
    .bind(i64::from(limit.min(1_000)))
    .execute(pool)
    .await?
    .rows_affected())
}
