use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{Sqlite, SqliteExecutor, SqlitePool, Transaction};

use crate::domain::commerce::order::{Fulfillment, Order, OrderChannel, TransitionOrder};
use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow)]
struct OrderRow {
    id: String,
    public_id: String,
    establishment_id: String,
    order_number: i64,
    business_date: NaiveDate,
    channel: String,
    fulfillment: String,
    state: String,
    version: i64,
    catalog_channel: String,
    catalog_version: i64,
    items_subtotal_centavos: i64,
    discount_centavos: i64,
    delivery_fee_centavos: i64,
    total_centavos: i64,
    placed_at: DateTime<Utc>,
    acceptance_deadline_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    accepted_at: Option<DateTime<Utc>>,
    promised_at: Option<DateTime<Utc>>,
    preparing_at: Option<DateTime<Utc>>,
    ready_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
    reason_code: Option<String>,
}

impl TryFrom<OrderRow> for Order {
    type Error = AppError;

    fn try_from(row: OrderRow) -> AppResult<Self> {
        Ok(Self {
            id: row.id,
            public_id: row.public_id,
            establishment_id: row.establishment_id,
            order_number: row.order_number,
            business_date: row.business_date,
            channel: OrderChannel::parse(&row.channel)
                .ok_or_else(|| AppError::Internal("invalid stored order channel".into()))?,
            fulfillment: Fulfillment::parse(&row.fulfillment)
                .ok_or_else(|| AppError::Internal("invalid stored order fulfillment".into()))?,
            state: row
                .state
                .parse()
                .map_err(|_| AppError::Internal("invalid stored order state".into()))?,
            version: row.version,
            catalog_channel: row.catalog_channel,
            catalog_version: row.catalog_version,
            items_subtotal_centavos: row.items_subtotal_centavos,
            discount_centavos: row.discount_centavos,
            delivery_fee_centavos: row.delivery_fee_centavos,
            total_centavos: row.total_centavos,
            placed_at: row.placed_at,
            acceptance_deadline_at: row.acceptance_deadline_at,
            updated_at: row.updated_at,
            accepted_at: row.accepted_at,
            promised_at: row.promised_at,
            preparing_at: row.preparing_at,
            ready_at: row.ready_at,
            completed_at: row.completed_at,
            rejected_at: row.rejected_at,
            cancelled_at: row.cancelled_at,
            reason_code: row.reason_code,
        })
    }
}

pub async fn get<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
    id: &str,
) -> AppResult<Option<Order>> {
    sqlx::query_as::<_, OrderRow>("SELECT * FROM orders WHERE id = ? AND establishment_id = ?")
        .bind(id)
        .bind(establishment_id)
        .fetch_optional(executor)
        .await?
        .map(Order::try_from)
        .transpose()
}

#[derive(sqlx::FromRow)]
pub(crate) struct ExpiredOrder {
    pub establishment_id: String,
    pub id: String,
    pub version: i64,
}

/// Platform maintenance scans the deadline index. Any subsequent write is tenant-scoped.
pub(crate) async fn expired_batch(
    pool: &SqlitePool,
    at: DateTime<Utc>,
) -> AppResult<Vec<ExpiredOrder>> {
    Ok(sqlx::query_as(
        "SELECT establishment_id, id, version FROM orders
         WHERE state = 'placed' AND acceptance_deadline_at <= ?
         ORDER BY acceptance_deadline_at, id LIMIT 32",
    )
    .bind(at)
    .fetch_all(pool)
    .await?)
}

pub(crate) async fn next_number(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    business_date: NaiveDate,
) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO order_number_counters (establishment_id, business_date, last_number)
         VALUES (?1, ?2, 1)
         ON CONFLICT (establishment_id, business_date) DO UPDATE SET last_number = last_number + 1
         WHERE establishment_id = ?1
         RETURNING last_number",
    )
    .bind(establishment_id)
    .bind(business_date)
    .fetch_one(&mut **tx)
    .await?)
}

pub(crate) async fn insert(tx: &mut Transaction<'_, Sqlite>, order: &Order) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO orders (
            id, establishment_id, public_id, order_number, business_date, channel, fulfillment,
            state, version, catalog_channel, catalog_version, items_subtotal_centavos,
            discount_centavos, delivery_fee_centavos, total_centavos, placed_at,
            acceptance_deadline_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&order.id)
    .bind(&order.establishment_id)
    .bind(&order.public_id)
    .bind(order.order_number)
    .bind(order.business_date)
    .bind(order.channel.as_str())
    .bind(order.fulfillment.as_str())
    .bind(order.state.as_str())
    .bind(order.version)
    .bind(&order.catalog_channel)
    .bind(order.catalog_version)
    .bind(order.items_subtotal_centavos)
    .bind(order.discount_centavos)
    .bind(order.delivery_fee_centavos)
    .bind(order.total_centavos)
    .bind(order.placed_at)
    .bind(order.acceptance_deadline_at)
    .bind(order.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn update_state(
    tx: &mut Transaction<'_, Sqlite>,
    order: &Order,
    command: TransitionOrder<'_>,
    at: DateTime<Utc>,
) -> AppResult<Option<Order>> {
    sqlx::query_as::<_, OrderRow>(
        "UPDATE orders SET
            state = ?1,
            version = version + 1,
            updated_at = ?2,
            accepted_at = CASE WHEN ?1 = 'accepted' THEN ?2 ELSE accepted_at END,
            promised_at = COALESCE(?3, promised_at),
            preparing_at = CASE WHEN ?1 = 'preparing' THEN ?2 ELSE preparing_at END,
            ready_at = CASE WHEN ?1 = 'ready' THEN ?2 ELSE ready_at END,
            completed_at = CASE WHEN ?1 = 'completed' THEN ?2 ELSE completed_at END,
            rejected_at = CASE WHEN ?1 = 'rejected' THEN ?2 ELSE rejected_at END,
            cancelled_at = CASE WHEN ?1 = 'cancelled' THEN ?2 ELSE cancelled_at END,
            reason_code = COALESCE(?4, reason_code)
         WHERE id = ?5 AND establishment_id = ?6 AND state = ?7 AND version = ?8
         RETURNING *",
    )
    .bind(command.action.target().as_str())
    .bind(at)
    .bind(command.action.promised_at())
    .bind(command.action.reason_code())
    .bind(&order.id)
    .bind(&order.establishment_id)
    .bind(command.expected_state.as_str())
    .bind(command.expected_version)
    .fetch_optional(&mut **tx)
    .await?
    .map(Order::try_from)
    .transpose()
}
