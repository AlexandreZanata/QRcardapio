pub mod order_creation;
pub mod order_transitions;

use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use crate::domain::commerce::order::{Order, OrderEventContext};
use crate::domain::events::{EventType, NewEvent};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::domain_events;

async fn append_order_event(
    tx: &mut Transaction<'_, Sqlite>,
    order: &Order,
    event_type: EventType,
    payload: Value,
    context: OrderEventContext,
    dedupe_key: String,
) -> AppResult<()> {
    let mut event = NewEvent::new(event_type, &order.establishment_id, &order.id, payload)
        .correlated(context.correlation_id.map(|id| id.to_string()))
        .deduplicated(dedupe_key);
    event.causation_id = context.causation_id.map(|id| id.to_string());
    if !domain_events::append(tx, &event).await? {
        return Err(AppError::Conflict("commerce.order_event_conflict"));
    }
    Ok(())
}
