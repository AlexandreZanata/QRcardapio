use chrono::Utc;
use sqlx::{Sqlite, SqliteExecutor, Transaction};
use uuid::Uuid;

use crate::domain::events::NewEvent;
use crate::error::AppResult;

/// Appends an event inside the caller's transaction. Returns `false` when an event with the
/// same `dedupe_key` already exists, so producers stay idempotent across retries.
pub async fn append(tx: &mut Transaction<'_, Sqlite>, event: &NewEvent) -> AppResult<bool> {
    let event_id = Uuid::now_v7().to_string();
    let correlation = event
        .correlation_id
        .clone()
        .unwrap_or_else(|| event_id.clone());
    let inserted = sqlx::query(
        "INSERT INTO domain_events (
            event_id, event_type, event_version, establishment_id, aggregate_type, aggregate_id,
            correlation_id, causation_id, occurred_at, payload, dedupe_key
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(dedupe_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(event.event_type.as_str())
    .bind(i64::from(event.event_type.version()))
    .bind(&event.establishment_id)
    .bind(event.event_type.aggregate_type())
    .bind(&event.aggregate_id)
    .bind(correlation)
    .bind(event.causation_id.as_deref())
    .bind(Utc::now().to_rfc3339())
    .bind(event.payload.to_string())
    .bind(event.dedupe_key.as_deref())
    .execute(&mut **tx)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredEvent {
    pub seq: i64,
    pub event_id: String,
    pub event_type: String,
    pub event_version: i64,
    pub establishment_id: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: String,
    pub payload: String,
}

pub(crate) async fn event_type<'e>(
    executor: impl SqliteExecutor<'e>,
    event_id: &str,
) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT event_type FROM domain_events WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(executor)
            .await?,
    )
}

/// Events of one establishment in outbox order (audit and future consumers).
pub async fn list_for_establishment<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
) -> AppResult<Vec<StoredEvent>> {
    Ok(sqlx::query_as(
        "SELECT seq, event_id, event_type, event_version, establishment_id, aggregate_type,
                aggregate_id, correlation_id, causation_id, occurred_at, payload
         FROM domain_events WHERE establishment_id = ? ORDER BY seq ASC",
    )
    .bind(establishment_id)
    .fetch_all(executor)
    .await?)
}
