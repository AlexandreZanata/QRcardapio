use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::append_order_event;
use crate::domain::commerce::order::{
    CancelReason, Order, OrderAction, RejectReason, TransitionOrder,
};
use crate::domain::commerce::state_machine::{Actor, Machine, transition as machine_transition};
use crate::domain::events::EventType;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::database::queries::{order_transitions, orders};

const CONFLICT: AppError = AppError::Conflict("commerce.order_state_conflict");

/// Applies a transition for a previously authorized channel operation. The transaction
/// checks staff membership and the machine rules, plus order-owned timestamps. Channel
/// and consumer identity verification belong to the caller exposing the operation.
pub async fn transition(
    pool: &SqlitePool,
    establishment_id: &str,
    order_id: &str,
    command: TransitionOrder<'_>,
    at: DateTime<Utc>,
) -> AppResult<Order> {
    let mut tx = begin_immediate(pool).await?;
    let order = orders::get(&mut *tx, establishment_id, order_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    validate_actor(&mut tx, establishment_id, command).await?;
    if order.state != command.expected_state || order.version != command.expected_version {
        return Err(CONFLICT);
    }

    let rule = machine_transition(
        Machine::Order,
        command.expected_state.into(),
        command.action.target().into(),
        command.actor,
    )
    .ok_or(CONFLICT)?;
    let event_type = rule
        .event
        .and_then(|event| event.implemented())
        .ok_or_else(|| AppError::Internal("order transition event is missing".into()))?;
    validate_preconditions(&order, command, at)?;

    let changed = orders::update_state(&mut tx, &order, command, at)
        .await?
        .ok_or(CONFLICT)?;
    order_transitions::append(&mut tx, &changed, command).await?;
    append_order_event(
        &mut tx,
        &changed,
        event_type,
        event_payload(event_type, &changed, command)?,
        command.event_context,
        format!(
            "{}:{}:{}",
            event_type.as_str(),
            changed.public_id,
            changed.version
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(changed)
}

async fn validate_actor(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    command: TransitionOrder<'_>,
) -> AppResult<()> {
    let allowed = match (command.actor, command.actor_account_id) {
        (Actor::Consumer | Actor::System, None) => true,
        (Actor::Consumer | Actor::System, Some(_)) | (Actor::Gateway, _) | (_, None) => false,
        (actor, Some(account_id)) => {
            order_transitions::has_member_role(tx, establishment_id, account_id, actor).await?
        }
    };
    if allowed {
        Ok(())
    } else {
        Err(AppError::Forbidden("error.forbidden"))
    }
}

fn validate_preconditions(
    order: &Order,
    command: TransitionOrder<'_>,
    at: DateTime<Utc>,
) -> AppResult<()> {
    if at < order.updated_at {
        return Err(CONFLICT);
    }
    match command.action {
        OrderAction::Accept { promised_at }
            if at >= order.acceptance_deadline_at || promised_at < at =>
        {
            Err(CONFLICT)
        }
        OrderAction::Reject { reason } => {
            let timeout = reason == RejectReason::AcceptanceTimeout;
            if timeout != (command.actor == Actor::System)
                || timeout && at < order.acceptance_deadline_at
            {
                return Err(CONFLICT);
            }
            Ok(())
        }
        OrderAction::Cancel { reason }
            if command.actor == Actor::Consumer && reason != CancelReason::ConsumerRequest =>
        {
            Err(CONFLICT)
        }
        _ => Ok(()),
    }
}

fn event_payload(
    event_type: EventType,
    order: &Order,
    command: TransitionOrder<'_>,
) -> AppResult<Value> {
    let order_id = &order.id;
    Ok(match (event_type, command.action) {
        (EventType::OrderAccepted, OrderAction::Accept { promised_at }) => {
            json!({ "order_id": order_id, "promised_at": promised_at.to_rfc3339() })
        }
        (EventType::OrderRejected, OrderAction::Reject { reason }) => {
            json!({ "order_id": order_id, "reason_code": reason.as_str() })
        }
        (EventType::OrderPreparing, OrderAction::Prepare) => json!({ "order_id": order_id }),
        (EventType::OrderReady, OrderAction::MarkReady) => {
            json!({ "order_id": order_id, "fulfillment": order.fulfillment.as_str() })
        }
        (EventType::OrderCompleted, OrderAction::Complete) => {
            json!({ "order_id": order_id, "completed_at": order.updated_at.to_rfc3339() })
        }
        (EventType::OrderCancelled, OrderAction::Cancel { reason }) => json!({
            "order_id": order_id,
            "reason_code": reason.as_str(),
            "cancelled_by": command.actor.as_str(),
        }),
        _ => {
            return Err(AppError::Internal(
                "order transition event does not match action".into(),
            ));
        }
    })
}
