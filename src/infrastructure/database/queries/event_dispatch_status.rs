use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;

const LAG_WARNING_SECONDS: i64 = 30;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ConsumerStatus {
    pub consumer: String,
    pub last_seq: i64,
    pub lag: i64,
    pub retry_count: i64,
    pub parked_count: i64,
    pub dead_letter_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DeadLetter {
    pub consumer: String,
    pub event_id: String,
    pub event_type: String,
    pub seq: i64,
    pub attempts: i64,
    pub error_key: String,
    pub replay_generation: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PendingStatus {
    pub pending_events: i64,
    pub oldest_pending_age_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DispatcherStatus {
    #[serde(flatten)]
    pub pending: PendingStatus,
    pub consumers: Vec<ConsumerStatus>,
    pub dead_letters: Vec<DeadLetter>,
}

pub async fn lag_warning(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM event_dispatch_publications p JOIN domain_events e ON e.seq = p.event_seq
             WHERE p.published_at IS NULL AND unixepoch(e.occurred_at) < ?
         ) OR EXISTS (SELECT 1 FROM event_consumer_failures WHERE status = 'dead')",
    )
    .bind(now.timestamp().saturating_sub(LAG_WARNING_SECONDS))
    .fetch_one(pool)
    .await?)
}

/// Operator metrics and dead-letter identifiers only. Event payloads never leave this layer.
/// A deferred read transaction makes counts and rows internally consistent without locking writers.
pub async fn status(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<DispatcherStatus> {
    let mut tx = pool.begin().await?;
    let pending = sqlx::query_as(
        "SELECT COUNT(*) AS pending_events, MAX(0, ? - MIN(unixepoch(e.occurred_at))) AS oldest_pending_age_seconds
         FROM event_dispatch_publications p JOIN domain_events e ON e.seq = p.event_seq
         WHERE p.published_at IS NULL",
    )
    .bind(now.timestamp())
    .fetch_one(&mut *tx)
    .await?;
    let consumers = sqlx::query_as(
        "SELECT c.consumer, c.last_seq,
                MAX(0, (SELECT COALESCE(MAX(seq), 0) FROM domain_events) - c.last_seq) AS lag,
                COUNT(*) FILTER (WHERE f.status = 'retry') AS retry_count,
                COUNT(*) FILTER (WHERE f.status = 'parked') AS parked_count,
                COUNT(*) FILTER (WHERE f.status = 'dead') AS dead_letter_count
         FROM event_consumer_checkpoints c LEFT JOIN event_consumer_failures f ON f.consumer = c.consumer
         GROUP BY c.consumer ORDER BY c.consumer",
    )
    .fetch_all(&mut *tx)
    .await?;
    let dead_letters = sqlx::query_as(
        "SELECT f.consumer, e.event_id, e.event_type, e.seq, f.attempts, f.error_key,
                COALESCE((
                    SELECT a.id FROM event_consumer_replays a
                    WHERE a.consumer = f.consumer AND a.event_seq = f.event_seq
                    ORDER BY a.id DESC LIMIT 1
                ), 0) AS replay_generation
         FROM event_consumer_failures f JOIN domain_events e ON e.seq = f.event_seq
         WHERE f.status = 'dead' ORDER BY e.seq DESC, f.consumer LIMIT 50",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(DispatcherStatus {
        pending,
        consumers,
        dead_letters,
    })
}
