use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::order_transitions::transition_in;
use crate::domain::commerce::order::{OrderAction, RejectReason, TransitionOrder};
use crate::domain::commerce::state_machine::{Actor, OrderState};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::orders;

/// Rejects one bounded batch of due orders. A staff transition that wins after the
/// scan supersedes the timer. Each rejection uses the ordinary history and outbox.
pub async fn expire_due(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<usize> {
    let expired = orders::expired_batch(pool, now).await?;
    if expired.is_empty() {
        return Ok(0);
    }
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
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
        match transition_in(
            &mut tx,
            &candidate.establishment_id,
            &candidate.id,
            command,
            now,
        )
        .await
        {
            Ok(_) => rejected += 1,
            Err(AppError::NotFound(_))
            | Err(AppError::Conflict("commerce.order_state_conflict")) => {}
            Err(error) => return Err(error),
        }
    }
    tx.commit().await?;
    if rejected > 0 {
        crate::infrastructure::events::dispatcher::notify_committed();
    }
    Ok(rejected)
}
