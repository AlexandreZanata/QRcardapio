use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::append_order_event;
use crate::domain::commerce::order::{LineError, NewOrder, Order, business_date, snapshot_lines};
use crate::domain::commerce::pricing::price_order;
use crate::domain::commerce::state_machine::{Machine, State, definition};
use crate::domain::events::EventType;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::database::queries::{catalog_snapshots, order_items, orders};

/// Internal order placement primitive. Channel authorization, quoting and idempotency
/// belong to the checkout service. Only the caller-supplied business rules and IDs carry in.
pub async fn create(
    pool: &SqlitePool,
    establishment_id: &str,
    command: &NewOrder,
    placed_at: DateTime<Utc>,
) -> AppResult<Order> {
    if command.acceptance_deadline_at <= placed_at {
        return Err(AppError::Validation("commerce.invalid_acceptance_deadline"));
    }

    let mut public_id = [0u8; 16];
    getrandom::fill(&mut public_id)
        .map_err(|error| AppError::Internal(format!("order id generation failed: {error}")))?;
    let business_date =
        business_date(placed_at).ok_or(AppError::Validation("commerce.invalid_order_date"))?;
    let mut tx = begin_immediate(pool).await?;
    let catalog =
        catalog_snapshots::current_in(&mut tx, establishment_id, &command.catalog_channel)
            .await?
            .filter(|snapshot| snapshot.version == command.catalog_version)
            .ok_or(AppError::Conflict("commerce.checkout_repriced"))?;
    let content = catalog.decode()?;
    let items = snapshot_lines(&content, &command.items).map_err(|error| match error {
        LineError::InvalidSelection => AppError::Conflict("commerce.checkout_repriced"),
        LineError::Price => AppError::Validation("commerce.invalid_order_price"),
    })?;
    let lines: Vec<_> = items.iter().map(|line| line.price).collect();
    let totals = price_order(&lines, command.discount, command.delivery_fee_centavos)
        .map_err(|_| AppError::Validation("commerce.invalid_order_price"))?;
    let State::Order(initial_state) = definition(Machine::Order).initial else {
        return Err(AppError::Internal(
            "order machine initial state is invalid".into(),
        ));
    };
    let order = Order {
        id: Uuid::now_v7().to_string(),
        public_id: hex::encode(public_id),
        establishment_id: establishment_id.to_owned(),
        order_number: orders::next_number(&mut tx, establishment_id, business_date).await?,
        business_date,
        channel: command.channel,
        fulfillment: command.fulfillment,
        state: initial_state,
        version: 1,
        catalog_channel: command.catalog_channel.clone(),
        catalog_version: catalog.version,
        items_subtotal_centavos: totals.items_subtotal_centavos,
        discount_centavos: totals.discount_centavos,
        delivery_fee_centavos: totals.delivery_fee_centavos,
        total_centavos: totals.total_centavos,
        placed_at,
        acceptance_deadline_at: command.acceptance_deadline_at,
        updated_at: placed_at,
        accepted_at: None,
        promised_at: None,
        preparing_at: None,
        ready_at: None,
        completed_at: None,
        rejected_at: None,
        cancelled_at: None,
        reason_code: None,
    };
    orders::insert(&mut tx, &order).await?;
    order_items::insert(&mut tx, &order, &items).await?;
    append_order_event(
        &mut tx,
        &order,
        EventType::OrderCreated,
        json!({
            "order_id": order.id,
            "channel": order.channel.as_str(),
            "total_centavos": order.total_centavos,
            "item_count": items.len(),
            "catalog_version": order.catalog_version,
        }),
        command.event_context,
        order.public_id.clone(),
    )
    .await?;
    tx.commit().await?;
    Ok(order)
}
