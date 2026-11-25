mod compensation;
mod transition;

pub use compensation::compensate_cancelled_order_in;
pub use transition::{cancel, confirm, refund};

use chrono::{DateTime, Utc};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::domain::commerce::order::Order;
use crate::domain::commerce::order_payment::{OfflinePaymentChoice, OrderPayment};
use crate::domain::commerce::state_machine::{Machine, OrderState, State, definition};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{order_payments, orders};

const CONFLICT: AppError = AppError::Conflict("commerce.payment_state_conflict");

/// Internal operation for channels that place collection intents separately from an order.
pub async fn create_pending(
    pool: &SqlitePool,
    establishment_id: &str,
    order_id: &str,
    choice: OfflinePaymentChoice,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let payment = create_for(&mut tx, establishment_id, order_id, choice, at).await?;
    tx.commit().await?;
    Ok(payment)
}

/// Inserts a collection intent in the order's placement transaction. The amount
/// comes from the persisted order. Checkout is responsible for channel authorization.
pub async fn create_pending_in(
    tx: &mut Transaction<'_, Sqlite>,
    order: &Order,
    choice: OfflinePaymentChoice,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    create_for(tx, &order.establishment_id, &order.id, choice, at).await
}

async fn create_for(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    order_id: &str,
    choice: OfflinePaymentChoice,
    at: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    let order = orders::get(&mut **tx, establishment_id, order_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    if !may_collect(order.state) || at < order.placed_at {
        return Err(CONFLICT);
    }
    if !choice.accepts_change(order.total_centavos) {
        return Err(AppError::Validation("commerce.invalid_cash_change"));
    }
    if order_payments::current(&mut **tx, establishment_id, order_id)
        .await?
        .is_some()
    {
        return Err(CONFLICT);
    }
    let State::OrderPayment(initial) = definition(Machine::OrderPayment).initial else {
        return Err(AppError::Internal(
            "invalid payment machine initial state".into(),
        ));
    };
    let payment = OrderPayment {
        id: Uuid::now_v7().to_string(),
        public_id: Uuid::new_v4().to_string(),
        establishment_id: order.establishment_id,
        order_id: order.id,
        requested_method: choice.method,
        method: choice.method,
        state: initial,
        version: 1,
        amount_centavos: order.total_centavos,
        refunded_centavos: 0,
        change_for_centavos: choice.change_for_centavos,
        refund_due_centavos: 0,
        refund_due_at: None,
        refund_due_event_id: None,
        created_at: at,
        updated_at: at,
    };
    order_payments::insert(tx, &payment).await?;
    Ok(payment)
}

fn may_collect(state: OrderState) -> bool {
    !matches!(state, OrderState::Rejected | OrderState::Cancelled)
}
