use serde_json::json;
use sqlx::{Sqlite, SqliteExecutor, SqlitePool, Transaction};

use super::domain_events::StoredEvent;
use crate::error::AppResult;

pub async fn latest_seq<'e>(executor: impl SqliteExecutor<'e>) -> AppResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM domain_events")
            .fetch_one(executor)
            .await?,
    )
}

pub async fn checkpoint<'e>(
    executor: impl SqliteExecutor<'e>,
    consumer: &str,
) -> AppResult<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT last_seq FROM event_consumer_checkpoints WHERE consumer = ?")
            .bind(consumer)
            .fetch_optional(executor)
            .await?,
    )
}

pub async fn advance(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    last_seq: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO event_consumer_checkpoints (consumer, last_seq) VALUES (?, ?)
         ON CONFLICT (consumer) DO UPDATE SET last_seq = excluded.last_seq
         WHERE excluded.last_seq > last_seq",
    )
    .bind(consumer)
    .bind(last_seq)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// The earliest unresolved event of each aggregate is the only one eligible to move.
// The tenant participates in the aggregate identity even when existing producers use UUIDs.
macro_rules! earlier_failure {
    ($consumer:literal) => {
        concat!(
            "SELECT 1 FROM domain_events earlier
             JOIN event_consumer_failures blocking ON blocking.event_seq = earlier.seq
             WHERE blocking.consumer = ",
            $consumer,
            " AND earlier.seq < e.seq AND earlier.aggregate_type = e.aggregate_type
              AND earlier.aggregate_id = e.aggregate_id AND earlier.establishment_id = e.establishment_id"
        )
    };
}

macro_rules! receipt_exists {
    () => {
        "EXISTS (SELECT 1 FROM event_consumer_receipts r
                 WHERE r.consumer = ?1 AND r.event_id = e.event_id)"
    };
}

pub(crate) async fn has_work(
    pool: &SqlitePool,
    subscriptions_json: &str,
    now_ms: i64,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(concat!(
        "WITH subscribers AS MATERIALIZED (
            SELECT value ->> 'consumer' AS name FROM json_each(?1)
         )
         SELECT EXISTS (
            SELECT 1 FROM subscribers s
            LEFT JOIN event_consumer_checkpoints c ON c.consumer = s.name
            WHERE c.last_seq IS NULL OR c.last_seq < (SELECT MAX(seq) FROM domain_events)
         ) OR EXISTS (
            SELECT 1 FROM subscribers s
            JOIN event_consumer_failures f ON f.consumer = s.name
            JOIN domain_events e ON e.seq = f.event_seq
            WHERE (f.status = 'parked' OR f.next_attempt_at <= ?2) AND NOT EXISTS (",
        earlier_failure!("s.name"),
        "))"
    ))
    .bind(subscriptions_json)
    .bind(now_ms)
    .fetch_one(pool)
    .await?)
}

#[derive(sqlx::FromRow)]
pub(crate) struct ScannedEvent {
    #[sqlx(flatten)]
    pub event: StoredEvent,
    pub received: bool,
    pub blocked: bool,
}

pub(crate) async fn unscanned(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    types_json: &str,
    after: i64,
    through: i64,
    limit: i64,
) -> AppResult<Vec<ScannedEvent>> {
    // Scan rowids in order. A type-first index would re-sort the remaining backlog.
    Ok(sqlx::query_as(concat!(
        "SELECT e.seq, e.event_id, e.event_type, e.event_version, e.establishment_id,
         e.aggregate_type, e.aggregate_id, e.correlation_id, e.causation_id, e.occurred_at,
         e.payload, ",
        receipt_exists!(),
        " AS received, CASE WHEN EXISTS (
             SELECT 1 FROM event_consumer_failures WHERE consumer = ?1
         ) THEN EXISTS (",
        earlier_failure!("?1"),
        ") ELSE 0 END AS blocked
         FROM domain_events e NOT INDEXED
         WHERE e.event_type IN (SELECT value FROM json_each(?2)) AND e.seq > ?3 AND e.seq <= ?4
         ORDER BY e.seq LIMIT ?5"
    ))
    .bind(consumer)
    .bind(types_json)
    .bind(after)
    .bind(through)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?)
}

#[derive(sqlx::FromRow)]
pub(crate) struct PendingEvent {
    #[sqlx(flatten)]
    pub event: StoredEvent,
    pub attempts: i64,
    pub received: bool,
}

pub(crate) async fn due_retries(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    now_ms: i64,
    limit: i64,
) -> AppResult<Vec<PendingEvent>> {
    Ok(sqlx::query_as(concat!(
        "SELECT e.seq, e.event_id, e.event_type, e.event_version, e.establishment_id,
         e.aggregate_type, e.aggregate_id, e.correlation_id, e.causation_id, e.occurred_at,
         e.payload, f.attempts, ",
        receipt_exists!(),
        " AS received FROM event_consumer_failures f JOIN domain_events e ON e.seq = f.event_seq
         WHERE f.consumer = ?1 AND (f.status = 'parked' OR f.next_attempt_at <= ?2)
         AND NOT EXISTS (",
        earlier_failure!("?1"),
        ") ORDER BY e.seq LIMIT ?3"
    ))
    .bind(consumer)
    .bind(now_ms)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn park(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    event_seq: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO event_consumer_failures (consumer, event_seq, status, attempts)
         VALUES (?, ?, 'parked', 0) ON CONFLICT (consumer, event_seq) DO NOTHING",
    )
    .bind(consumer)
    .bind(event_seq)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn record_failure(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    event_seq: i64,
    attempts: i64,
    retry_at_ms: Option<i64>,
    error_key: &str,
) -> AppResult<()> {
    let status = if retry_at_ms.is_some() {
        "retry"
    } else {
        "dead"
    };
    sqlx::query(
        "INSERT INTO event_consumer_failures (
            consumer, event_seq, status, attempts, next_attempt_at, error_key
         ) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (consumer, event_seq) DO UPDATE SET status = excluded.status,
            attempts = excluded.attempts, next_attempt_at = excluded.next_attempt_at,
            error_key = excluded.error_key",
    )
    .bind(consumer)
    .bind(event_seq)
    .bind(status)
    .bind(attempts)
    .bind(retry_at_ms)
    .bind(error_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn clear_failure(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    event_seq: i64,
) -> AppResult<()> {
    sqlx::query("DELETE FROM event_consumer_failures WHERE consumer = ? AND event_seq = ?")
        .bind(consumer)
        .bind(event_seq)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Records successful handlers together before committing their write transaction.
pub(crate) async fn record_receipts(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    event_ids: &[&str],
    now_ms: i64,
) -> AppResult<()> {
    if event_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO event_consumer_receipts (consumer, event_id, processed_at)
         SELECT ?1, value, ?3 FROM json_each(?2)",
    )
    .bind(consumer)
    .bind(json!(event_ids).to_string())
    .bind(now_ms)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
