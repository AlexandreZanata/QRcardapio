use chrono::Utc;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::infrastructure::database::commerce_writes;

// The low-water mark for a particular event also respects outstanding deliveries in the
// retry/parking queue. Mere checkpoint advancement cannot acknowledge a failed handler.
macro_rules! publishable_events {
    () => {
        "SELECT p.event_seq FROM event_dispatch_publications p
         JOIN domain_events e ON e.seq = p.event_seq
         WHERE p.published_at IS NULL
           AND NOT EXISTS (
                SELECT 1 FROM json_each(?1) subscription
                JOIN json_each(subscription.value, '$.event_types') kind
                WHERE kind.value = e.event_type AND NOT EXISTS (
                    SELECT 1 FROM event_consumer_receipts r WHERE r.event_id = e.event_id
                      AND r.consumer = subscription.value ->> 'consumer'
                )
           )"
    };
}

pub async fn mark_ready(
    pool: &SqlitePool,
    subscriptions_json: &str,
    now_ms: i64,
    limit: i64,
) -> AppResult<usize> {
    let pending: bool = sqlx::query_scalar(concat!("SELECT EXISTS (", publishable_events!(), ")"))
        .bind(subscriptions_json)
        .fetch_one(pool)
        .await?;
    if !pending {
        return Ok(0);
    }

    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let changed = sqlx::query(concat!(
        "UPDATE event_dispatch_publications SET published_at = ?2 WHERE event_seq IN (",
        publishable_events!(),
        " ORDER BY p.event_seq LIMIT ?3)"
    ))
    .bind(subscriptions_json)
    .bind(now_ms.max(Utc::now().timestamp_millis()))
    .bind(limit)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(changed.rows_affected() as usize)
}
