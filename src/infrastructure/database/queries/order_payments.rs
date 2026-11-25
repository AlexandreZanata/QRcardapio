use chrono::{DateTime, Utc};
use sqlx::{Sqlite, SqliteExecutor, Transaction};

use crate::domain::commerce::order_payment::{OfflineMethod, OrderPayment};
use crate::domain::commerce::state_machine::{Actor, OrderPaymentState};
use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow)]
struct PaymentRow {
    id: String,
    public_id: String,
    establishment_id: String,
    order_id: String,
    requested_method: String,
    method: String,
    state: String,
    version: i64,
    amount_centavos: i64,
    refunded_centavos: i64,
    change_for_centavos: Option<i64>,
    refund_due_centavos: i64,
    refund_due_at: Option<DateTime<Utc>>,
    refund_due_event_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<PaymentRow> for OrderPayment {
    type Error = AppError;

    fn try_from(row: PaymentRow) -> AppResult<Self> {
        let method = |value| {
            OfflineMethod::parse(value)
                .ok_or_else(|| AppError::Internal("invalid stored offline payment method".into()))
        };
        Ok(Self {
            id: row.id,
            public_id: row.public_id,
            establishment_id: row.establishment_id,
            order_id: row.order_id,
            requested_method: method(&row.requested_method)?,
            method: method(&row.method)?,
            state: row
                .state
                .parse()
                .map_err(|_| AppError::Internal("invalid stored offline payment state".into()))?,
            version: row.version,
            amount_centavos: row.amount_centavos,
            refunded_centavos: row.refunded_centavos,
            change_for_centavos: row.change_for_centavos,
            refund_due_centavos: row.refund_due_centavos,
            refund_due_at: row.refund_due_at,
            refund_due_event_id: row.refund_due_event_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

pub async fn get<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
    order_id: &str,
    id: &str,
) -> AppResult<Option<OrderPayment>> {
    sqlx::query_as::<_, PaymentRow>(
        "SELECT * FROM order_payments WHERE id = ? AND order_id = ? AND establishment_id = ?",
    )
    .bind(id)
    .bind(order_id)
    .bind(establishment_id)
    .fetch_optional(executor)
    .await?
    .map(OrderPayment::try_from)
    .transpose()
}

pub async fn current<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
    order_id: &str,
) -> AppResult<Option<OrderPayment>> {
    sqlx::query_as::<_, PaymentRow>(
        "SELECT * FROM order_payments
         WHERE order_id = ? AND establishment_id = ? AND state <> 'cancelled'",
    )
    .bind(order_id)
    .bind(establishment_id)
    .fetch_optional(executor)
    .await?
    .map(OrderPayment::try_from)
    .transpose()
}

pub(crate) async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    payment: &OrderPayment,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO order_payments (
            id, public_id, establishment_id, order_id, requested_method, method,
            state, amount_centavos, change_for_centavos, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&payment.id)
    .bind(&payment.public_id)
    .bind(&payment.establishment_id)
    .bind(&payment.order_id)
    .bind(payment.requested_method.as_str())
    .bind(payment.method.as_str())
    .bind(payment.state.as_str())
    .bind(payment.amount_centavos)
    .bind(payment.change_for_centavos)
    .bind(payment.created_at)
    .bind(payment.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) struct PaymentUpdate {
    pub state: OrderPaymentState,
    pub method: OfflineMethod,
    pub refunded_centavos: i64,
    pub refund_due_centavos: i64,
    pub at: DateTime<Utc>,
}

pub(crate) async fn update(
    tx: &mut Transaction<'_, Sqlite>,
    before: &OrderPayment,
    change: PaymentUpdate,
) -> AppResult<Option<OrderPayment>> {
    sqlx::query_as::<_, PaymentRow>(
        "UPDATE order_payments SET state = ?, method = ?, refunded_centavos = ?,
            refund_due_centavos = ?, updated_at = ?, version = version + 1
         WHERE id = ? AND order_id = ? AND establishment_id = ? AND state = ? AND version = ?
         RETURNING *",
    )
    .bind(change.state.as_str())
    .bind(change.method.as_str())
    .bind(change.refunded_centavos)
    .bind(change.refund_due_centavos)
    .bind(change.at)
    .bind(&before.id)
    .bind(&before.order_id)
    .bind(&before.establishment_id)
    .bind(before.state.as_str())
    .bind(before.version)
    .fetch_optional(&mut **tx)
    .await?
    .map(OrderPayment::try_from)
    .transpose()
}

pub(crate) async fn mark_refund_due(
    tx: &mut Transaction<'_, Sqlite>,
    payment: &OrderPayment,
    event_id: &str,
    at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE order_payments SET refund_due_centavos = amount_centavos - refunded_centavos,
            refund_due_at = ?, refund_due_event_id = ?
         WHERE id = ? AND order_id = ? AND establishment_id = ? AND version = ?
           AND state IN ('confirmed', 'partially_refunded') AND amount_centavos > refunded_centavos
           AND refund_due_centavos = 0",
    )
    .bind(at)
    .bind(event_id)
    .bind(&payment.id)
    .bind(&payment.order_id)
    .bind(&payment.establishment_id)
    .bind(payment.version)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn append_history(
    tx: &mut Transaction<'_, Sqlite>,
    before: &OrderPayment,
    after: &OrderPayment,
    actor: Actor,
    actor_account_id: Option<&str>,
    causation_id: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO order_payment_transitions (
            establishment_id, payment_id, version, from_state, to_state,
            actor_kind, actor_account_id, causation_id, occurred_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&after.establishment_id)
    .bind(&after.id)
    .bind(after.version)
    .bind(before.state.as_str())
    .bind(after.state.as_str())
    .bind(actor.as_str())
    .bind(actor_account_id)
    .bind(causation_id)
    .bind(after.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
