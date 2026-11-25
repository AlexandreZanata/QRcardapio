use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use super::{CONFLICT, may_collect};
use crate::domain::commerce::order_payment::{
    OfflineMethod, OrderPayment, PaymentOperation, PaymentRefund, RefundReason,
};
use crate::domain::commerce::state_machine::{Actor, Machine, OrderPaymentState, transition};
use crate::domain::events::{EventType, NewEvent};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{
    domain_events,
    order_payments::{self, PaymentUpdate},
    order_transitions, orders, payment_refunds,
};

#[derive(Clone, Copy)]
enum Action {
    Confirm(OfflineMethod),
    Cancel,
    Refund {
        amount_centavos: i64,
        reason: RefundReason,
    },
}

/// Staff records the collected method, which can differ from the buyer's request.
/// The caller authorizes the channel operation before invoking this ledger primitive.
pub async fn confirm(
    pool: &SqlitePool,
    command: PaymentOperation<'_>,
    method: OfflineMethod,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    change(pool, command, Action::Confirm(method), at).await
}

pub async fn cancel(
    pool: &SqlitePool,
    command: PaymentOperation<'_>,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    change(pool, command, Action::Cancel, at).await
}

/// Records a refund already made by staff. The ledger never moves money.
pub async fn refund(
    pool: &SqlitePool,
    command: PaymentOperation<'_>,
    amount_centavos: i64,
    reason: RefundReason,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    change(
        pool,
        command,
        Action::Refund {
            amount_centavos,
            reason,
        },
        at,
    )
    .await
}

async fn change(
    pool: &SqlitePool,
    command: PaymentOperation<'_>,
    action: Action,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let previous = order_payments::get(
        &mut *tx,
        command.establishment_id,
        command.order_id,
        command.payment_id,
    )
    .await?
    .ok_or(AppError::NotFound("error.not_found"))?;
    if !matches!(
        command.actor,
        Actor::Owner | Actor::Manager | Actor::Cashier | Actor::Waiter
    ) || !order_transitions::has_member_role(
        &mut tx,
        command.establishment_id,
        command.actor_account_id,
        command.actor,
    )
    .await?
    {
        return Err(AppError::Forbidden("error.forbidden"));
    }
    if previous.version != command.expected_version || at < previous.updated_at {
        return Err(CONFLICT);
    }
    if matches!(action, Action::Confirm(_)) {
        let order = orders::get(&mut *tx, command.establishment_id, command.order_id)
            .await?
            .ok_or(AppError::NotFound("error.not_found"))?;
        if !may_collect(order.state) || previous.amount_centavos != order.total_centavos {
            return Err(CONFLICT);
        }
    }

    let (update, refund) = build_update(&previous, action, at)?;
    let rule = transition(
        Machine::OrderPayment,
        previous.state.into(),
        update.state.into(),
        command.actor,
    )
    .ok_or(CONFLICT)?;
    let changed = order_payments::update(&mut tx, &previous, update)
        .await?
        .ok_or(CONFLICT)?;
    let causation_id = command.event_context.causation_id.map(|id| id.to_string());
    order_payments::append_history(
        &mut tx,
        &previous,
        &changed,
        command.actor,
        Some(command.actor_account_id),
        causation_id.as_deref(),
    )
    .await?;
    if let Some(refund) = &refund {
        payment_refunds::insert(&mut tx, &changed, refund).await?;
    }
    if let Some(event) = rule.event {
        let event_type = event.implemented().ok_or_else(|| {
            AppError::Internal("offline payment event is missing from the registry".into())
        })?;
        append_event(&mut tx, &changed, command, event_type, refund.as_ref()).await?;
    }
    tx.commit().await?;
    Ok(changed)
}

fn build_update(
    payment: &OrderPayment,
    action: Action,
    at: DateTime<Utc>,
) -> AppResult<(PaymentUpdate, Option<PaymentRefund>)> {
    let mut update = PaymentUpdate {
        state: payment.state,
        method: payment.method,
        refunded_centavos: payment.refunded_centavos,
        refund_due_centavos: payment.refund_due_centavos,
        at,
    };
    let refund = match action {
        Action::Confirm(method) => {
            update.state = OrderPaymentState::Confirmed;
            update.method = method;
            None
        }
        Action::Cancel => {
            update.state = OrderPaymentState::Cancelled;
            None
        }
        Action::Refund {
            amount_centavos,
            reason,
        } => {
            if amount_centavos <= 0 {
                return Err(AppError::Validation("commerce.invalid_refund_amount"));
            }
            update.refunded_centavos = payment
                .refunded_centavos
                .checked_add(amount_centavos)
                .filter(|&total| total <= payment.amount_centavos)
                .ok_or(AppError::Validation("commerce.invalid_refund_amount"))?;
            update.state = if update.refunded_centavos == payment.amount_centavos {
                OrderPaymentState::Refunded
            } else {
                OrderPaymentState::PartiallyRefunded
            };
            update.refund_due_centavos = payment
                .refund_due_centavos
                .saturating_sub(amount_centavos)
                .max(0);
            Some(PaymentRefund {
                id: Uuid::now_v7().to_string(),
                payment_id: payment.id.clone(),
                amount_centavos,
                reason,
                recorded_at: at,
            })
        }
    };
    Ok((update, refund))
}

async fn append_event(
    tx: &mut Transaction<'_, Sqlite>,
    payment: &OrderPayment,
    command: PaymentOperation<'_>,
    event_type: EventType,
    refund: Option<&PaymentRefund>,
) -> AppResult<()> {
    let (payload, dedupe) = match (event_type, refund) {
        (EventType::PaymentConfirmed, None) => (
            json!({
                "payment_id": payment.id,
                "order_id": payment.order_id,
                "amount_centavos": payment.amount_centavos,
                "method": payment.method.as_str(),
            }),
            format!("PaymentConfirmed:{}", payment.id),
        ),
        (EventType::PaymentRefunded, Some(refund)) => (
            json!({
                "payment_id": payment.id,
                "order_id": payment.order_id,
                "amount_centavos": refund.amount_centavos,
                "refund_id": refund.id,
            }),
            format!("PaymentRefunded:{}", refund.id),
        ),
        _ => {
            return Err(AppError::Internal(
                "payment event does not match action".into(),
            ));
        }
    };
    let mut event = NewEvent::new(event_type, &payment.establishment_id, &payment.id, payload)
        .correlated(
            command
                .event_context
                .correlation_id
                .map(|id| id.to_string()),
        )
        .deduplicated(dedupe);
    event.causation_id = command.event_context.causation_id.map(|id| id.to_string());
    if !domain_events::append(tx, &event).await? {
        return Err(CONFLICT);
    }
    Ok(())
}
