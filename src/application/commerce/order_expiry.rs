use chrono::{DateTime, Utc};
use sqlx::{Acquire, SqlitePool};

use super::order_transitions::transition_in;
use crate::domain::commerce::order::{OrderAction, RejectReason, TransitionOrder};
use crate::domain::commerce::state_machine::{Actor, OrderState};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{order_expiry_retries, orders};

const BATCH_SIZE: i64 = 32;
const RETRY_DELAY_MS: i64 = 30_000;

/// Rejects one bounded batch of due orders. A staff transition that wins after the
/// scan supersedes the timer. Failed orders roll back independently and retry later.
pub async fn expire_due(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<usize> {
    if orders::expired_batch(pool, now, 1).await?.is_empty() {
        return Ok(0);
    }
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let expired = orders::expired_batch(&mut *tx, now, BATCH_SIZE).await?;
    let mut rejected = 0;
    for candidate in expired {
        let command = TransitionOrder {
            expected_state: OrderState::Placed,
            expected_version: candidate.version,
            action: OrderAction::Reject {
                reason: RejectReason::AcceptanceTimeout,
            },
            actor: Actor::System,
            actor_account_id: None,
            event_context: Default::default(),
        };
        let mut attempt = tx.begin().await?;
        let result = transition_in(
            &mut attempt,
            &candidate.establishment_id,
            &candidate.id,
            command,
            now,
        )
        .await;
        let Err(error) = result else {
            attempt.commit().await?;
            rejected += 1;
            continue;
        };
        attempt.rollback().await?;
        if !matches!(
            error,
            AppError::NotFound(_) | AppError::Conflict("commerce.order_state_conflict")
        ) {
            order_expiry_retries::defer(
                &mut tx,
                &candidate.establishment_id,
                &candidate.id,
                now.timestamp_millis().saturating_add(RETRY_DELAY_MS),
            )
            .await?;
            tracing::warn!(
                order_id = %candidate.id,
                error_key = error.i18n_key(),
                "Commerce acceptance expiry deferred after failure"
            );
        }
    }
    tx.commit().await?;
    if rejected > 0 {
        crate::infrastructure::events::dispatcher::notify_committed();
    }
    Ok(rejected)
}
