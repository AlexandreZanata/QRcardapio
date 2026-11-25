use chrono::Utc;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::AppResult;

macro_rules! subscriptions {
    () => {
        "WITH subscriptions AS MATERIALIZED (
             SELECT s.value ->> 'consumer' AS consumer, kind.value AS event_type
             FROM json_each(?1) s JOIN json_each(s.value, '$.event_types') kind
         ) "
    };
}

// The low-water mark for a particular event also respects outstanding deliveries in the
// retry/parking queue. Mere checkpoint advancement cannot acknowledge a failed handler.
macro_rules! publishable_events {
    () => {
        "SELECT p.event_seq FROM event_dispatch_publications p
         CROSS JOIN domain_events e ON e.seq = p.event_seq
         WHERE p.published_at IS NULL
           AND NOT EXISTS (
                SELECT 1 FROM subscriptions s
                WHERE s.event_type = e.event_type AND NOT EXISTS (
                    SELECT 1 FROM event_consumer_receipts r WHERE r.event_id = e.event_id
                      AND r.consumer = s.consumer
                )
           )"
    };
}

pub(crate) async fn has_ready(pool: &SqlitePool, subscriptions_json: &str) -> AppResult<bool> {
    Ok(sqlx::query_scalar(concat!(
        subscriptions!(),
        "SELECT EXISTS (",
        publishable_events!(),
        ")"
    ))
    .bind(subscriptions_json)
    .fetch_one(pool)
    .await?)
}

pub(crate) async fn mark_ready(
    tx: &mut Transaction<'_, Sqlite>,
    subscriptions_json: &str,
    now_ms: i64,
    limit: i64,
) -> AppResult<usize> {
    let changed = sqlx::query(concat!(
        subscriptions!(),
        "UPDATE event_dispatch_publications SET published_at = ?2 WHERE event_seq IN (",
        publishable_events!(),
        " ORDER BY p.event_seq LIMIT ?3)"
    ))
    .bind(subscriptions_json)
    .bind(now_ms.max(Utc::now().timestamp_millis()))
    .bind(limit)
    .execute(&mut **tx)
    .await?;
    Ok(changed.rows_affected() as usize)
}
