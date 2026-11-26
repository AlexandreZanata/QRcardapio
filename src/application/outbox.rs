use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::commerce::worker::CONSUMERS;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{domain_events, event_replays};
use crate::infrastructure::events::dispatcher::notify_committed;

const REPLAY_CONFLICT: &str = "admin_outbox.replay_conflict";

/// Schedule the observed dead-letter generation for an authenticated platform admin.
/// The worker runs the handler later through the normal ordered dispatcher.
pub async fn schedule_replay(
    pool: &SqlitePool,
    actor_account_id: &str,
    consumer_name: &str,
    event_id: &str,
    generation: i64,
    now: DateTime<Utc>,
) -> AppResult<()> {
    let consumer = CONSUMERS
        .iter()
        .find(|consumer| consumer.name == consumer_name)
        .ok_or(AppError::Conflict(REPLAY_CONFLICT))?;
    let event_id = Uuid::parse_str(event_id)
        .map_err(|_| AppError::Conflict(REPLAY_CONFLICT))?
        .to_string();
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let event_type = domain_events::event_type(&mut *tx, &event_id)
        .await?
        .ok_or(AppError::Conflict(REPLAY_CONFLICT))?;
    if !consumer
        .event_types
        .iter()
        .any(|kind| kind.as_str() == event_type)
    {
        return Err(AppError::Conflict(REPLAY_CONFLICT));
    }
    if !event_replays::schedule(
        &mut tx,
        consumer.name,
        &event_id,
        generation,
        actor_account_id,
        now.timestamp_millis(),
    )
    .await?
    {
        return Err(AppError::Conflict(REPLAY_CONFLICT));
    }
    tx.commit().await?;
    notify_committed();
    Ok(())
}
