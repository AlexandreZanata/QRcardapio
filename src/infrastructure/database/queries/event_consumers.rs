use sqlx::{Sqlite, SqliteConnection, SqliteExecutor, Transaction};

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

pub async fn unscanned(
    tx: &mut Transaction<'_, Sqlite>,
    types_json: &str,
    after: i64,
    through: i64,
    limit: i64,
) -> AppResult<Vec<StoredEvent>> {
    // Scan the ordered rowid range directly. A type-first index would sort the
    // remaining subscribed backlog again for every limited batch.
    Ok(sqlx::query_as(
        "SELECT seq, event_id, event_type, event_version, establishment_id, aggregate_type,
                aggregate_id, correlation_id, causation_id, occurred_at, payload
         FROM domain_events NOT INDEXED
         WHERE event_type IN (SELECT value FROM json_each(?)) AND seq > ? AND seq <= ?
         ORDER BY seq LIMIT ?",
    )
    .bind(types_json)
    .bind(after)
    .bind(through)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?)
}

// The earliest unresolved event of each aggregate is the only one eligible to move.
// The tenant participates in the aggregate identity even when existing producers use UUIDs.
macro_rules! earlier_failure {
    () => {
        "SELECT 1 FROM domain_events earlier
         JOIN event_consumer_failures blocking ON blocking.event_seq = earlier.seq
         WHERE blocking.consumer = ?1 AND earlier.seq < e.seq
           AND earlier.aggregate_type = e.aggregate_type AND earlier.aggregate_id = e.aggregate_id
           AND earlier.establishment_id = e.establishment_id"
    };
}

macro_rules! due_query {
    () => {
        concat!(
            "FROM event_consumer_failures f JOIN domain_events e ON e.seq = f.event_seq
             WHERE f.consumer = ?1 AND (f.status = 'parked' OR f.next_attempt_at <= ?2)
             AND NOT EXISTS (",
            earlier_failure!(),
            ")"
        )
    };
}

pub async fn has_due_retry<'e>(
    executor: impl SqliteExecutor<'e>,
    consumer: &str,
    now_ms: i64,
) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar(concat!("SELECT EXISTS (SELECT 1 ", due_query!(), ")"))
            .bind(consumer)
            .bind(now_ms)
            .fetch_one(executor)
            .await?,
    )
}

#[derive(sqlx::FromRow)]
pub struct PendingEvent {
    #[sqlx(flatten)]
    pub event: StoredEvent,
    pub attempts: i64,
}

pub async fn due_retries(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    now_ms: i64,
    limit: i64,
) -> AppResult<Vec<PendingEvent>> {
    Ok(sqlx::query_as(concat!(
        "SELECT e.seq, e.event_id, e.event_type, e.event_version, e.establishment_id,
         e.aggregate_type, e.aggregate_id, e.correlation_id, e.causation_id, e.occurred_at,
         e.payload, f.attempts ",
        due_query!(),
        " ORDER BY e.seq LIMIT ?3"
    ))
    .bind(consumer)
    .bind(now_ms)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?)
}

pub async fn is_blocked(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    event_seq: i64,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(concat!(
        "SELECT EXISTS (",
        earlier_failure!(),
        ") FROM domain_events e WHERE e.seq = ?2"
    ))
    .bind(consumer)
    .bind(event_seq)
    .fetch_one(&mut **tx)
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

/// Claims receipt inside the same transaction and savepoint as handler writes.
/// Existing receipt means the handler was already committed and can safely be skipped.
pub async fn claim(
    conn: &mut SqliteConnection,
    consumer: &str,
    event_id: &str,
    now_ms: i64,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT INTO event_consumer_receipts (consumer, event_id, processed_at)
         VALUES (?, ?, ?) ON CONFLICT (consumer, event_id) DO NOTHING",
    )
    .bind(consumer)
    .bind(event_id)
    .bind(now_ms)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}
