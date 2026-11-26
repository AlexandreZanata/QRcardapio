use sqlx::{Acquire, Sqlite, Transaction};

use crate::error::AppResult;

pub(crate) async fn schedule(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &str,
    event_id: &str,
    expected_generation: i64,
    actor_account_id: &str,
    now_ms: i64,
) -> AppResult<bool> {
    let mut replay = tx.begin().await?;
    let event_seq: Option<i64> = sqlx::query_scalar(
        "INSERT INTO event_consumer_replays (
            consumer, event_seq, actor_account_id, requested_at, previous_error_key
         )
         SELECT f.consumer, f.event_seq, ?4, ?5, f.error_key
         FROM domain_events e JOIN event_consumer_failures f
           ON f.event_seq = e.seq AND f.consumer = ?1
         WHERE e.event_id = ?2 AND f.status = 'dead'
           AND NOT EXISTS (
                SELECT 1 FROM event_consumer_receipts r
                WHERE r.consumer = f.consumer AND r.event_id = e.event_id
           )
           AND ?3 = COALESCE((
                SELECT a.id FROM event_consumer_replays a
                WHERE a.consumer = f.consumer AND a.event_seq = f.event_seq
                ORDER BY a.id DESC LIMIT 1
           ), 0)
         RETURNING event_seq",
    )
    .bind(consumer)
    .bind(event_id)
    .bind(expected_generation)
    .bind(actor_account_id)
    .bind(now_ms)
    .fetch_optional(&mut *replay)
    .await?;
    let Some(event_seq) = event_seq else {
        replay.rollback().await?;
        return Ok(false);
    };

    // Keep the row in the pending queue so its successors stay blocked until delivery.
    let changed = sqlx::query(
        "UPDATE event_consumer_failures
         SET status = 'parked', attempts = 0, next_attempt_at = NULL, error_key = NULL
         WHERE consumer = ? AND event_seq = ? AND status = 'dead'",
    )
    .bind(consumer)
    .bind(event_seq)
    .execute(&mut *replay)
    .await?;
    if changed.rows_affected() == 0 {
        replay.rollback().await?;
        return Ok(false);
    }
    replay.commit().await?;
    Ok(true)
}
