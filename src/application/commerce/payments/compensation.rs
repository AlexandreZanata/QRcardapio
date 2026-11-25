use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{Sqlite, Transaction};

use super::CONFLICT;
use crate::domain::commerce::state_machine::{
    Actor, Machine, OrderPaymentState, OrderState, can_transition,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    domain_events::StoredEvent,
    order_payments::{self, PaymentUpdate},
    orders,
};

#[derive(Deserialize)]
struct CancelledOrder {
    order_id: String,
}

/// Handles an order's terminal cancellation or rejection inside the dispatcher transaction.
/// Staff resolve offline refund dues later. A receipt and the effects can commit together.
pub async fn compensate_cancelled_order_in(
    tx: &mut Transaction<'_, Sqlite>,
    event: &StoredEvent,
) -> AppResult<()> {
    let expected = match event.event_type.as_str() {
        "OrderCancelled" => OrderState::Cancelled,
        "OrderRejected" => OrderState::Rejected,
        _ => {
            return Err(AppError::Internal(
                "unsupported payment compensation event".into(),
            ));
        }
    };
    let payload: CancelledOrder = serde_json::from_str(&event.payload)
        .map_err(|_| AppError::Internal("invalid payment compensation payload".into()))?;
    if event.aggregate_type != "order"
        || event.event_version != 1
        || payload.order_id != event.aggregate_id
    {
        return Err(AppError::Internal(
            "invalid payment compensation event envelope".into(),
        ));
    }

    let Some(order) = orders::get(&mut **tx, &event.establishment_id, &payload.order_id).await?
    else {
        return Ok(());
    };
    if order.state != expected {
        return Err(AppError::Internal(
            "payment compensation order state mismatch".into(),
        ));
    }
    let Some(payment) =
        order_payments::current(&mut **tx, &event.establishment_id, &order.id).await?
    else {
        return Ok(());
    };
    let occurred_at = DateTime::parse_from_rfc3339(&event.occurred_at)
        .map_err(|_| AppError::Internal("invalid payment compensation timestamp".into()))?
        .with_timezone(&Utc);
    match payment.state {
        OrderPaymentState::Pending => {
            let state = OrderPaymentState::Cancelled;
            if !can_transition(
                Machine::OrderPayment,
                payment.state.into(),
                state.into(),
                Actor::System,
            ) {
                return Err(CONFLICT);
            }
            let update = PaymentUpdate {
                state,
                method: payment.method,
                refunded_centavos: payment.refunded_centavos,
                refund_due_centavos: 0,
                at: occurred_at.max(payment.updated_at),
            };
            let changed = order_payments::update(tx, &payment, update)
                .await?
                .ok_or(CONFLICT)?;
            order_payments::append_history(
                tx,
                &payment,
                &changed,
                Actor::System,
                None,
                Some(&event.event_id),
            )
            .await?;
        }
        OrderPaymentState::Confirmed | OrderPaymentState::PartiallyRefunded => {
            order_payments::mark_refund_due(tx, &payment, &event.event_id, occurred_at).await?;
        }
        _ => {}
    }
    Ok(())
}
