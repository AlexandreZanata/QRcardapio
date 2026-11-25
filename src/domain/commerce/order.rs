mod lines;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::discount::Discount;
use super::state_machine::{Actor, OrderState};

pub(crate) use lines::{LineError, OrderItemSnapshot, assess_lines, snapshot_lines};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderChannel {
    Delivery,
    Pickup,
    Table,
    Counter,
    Manual,
    External,
}

impl OrderChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Delivery => "delivery",
            Self::Pickup => "pickup",
            Self::Table => "table",
            Self::Counter => "counter",
            Self::Manual => "manual",
            Self::External => "external",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "delivery" => Self::Delivery,
            "pickup" => Self::Pickup,
            "table" => Self::Table,
            "counter" => Self::Counter,
            "manual" => Self::Manual,
            "external" => Self::External,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fulfillment {
    Delivery,
    Pickup,
    InVenue,
}

impl Fulfillment {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Delivery => "delivery",
            Self::Pickup => "pickup",
            Self::InVenue => "in_venue",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "delivery" => Self::Delivery,
            "pickup" => Self::Pickup,
            "in_venue" => Self::InVenue,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub id: String,
    pub public_id: String,
    pub establishment_id: String,
    pub order_number: i64,
    pub business_date: NaiveDate,
    pub channel: OrderChannel,
    pub fulfillment: Fulfillment,
    pub state: OrderState,
    pub version: i64,
    pub catalog_channel: String,
    pub catalog_version: i64,
    pub items_subtotal_centavos: i64,
    pub discount_centavos: i64,
    pub delivery_fee_centavos: i64,
    pub total_centavos: i64,
    pub placed_at: DateTime<Utc>,
    pub acceptance_deadline_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub promised_at: Option<DateTime<Utc>>,
    pub preparing_at: Option<DateTime<Utc>>,
    pub ready_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub reason_code: Option<String>,
}

/// Numbering is based on the calendar date in São Paulo at placement.
pub fn business_date(placed_at: DateTime<Utc>) -> Option<NaiveDate> {
    let timestamp = jiff::Timestamp::new(
        placed_at.timestamp(),
        i32::try_from(placed_at.timestamp_subsec_nanos()).ok()?,
    )
    .ok()?;
    let date = timestamp
        .to_zoned_datetime(jiff::tz::get!("America/Sao_Paulo"))
        .date();
    NaiveDate::from_ymd_opt(
        i32::from(date.year()),
        u32::try_from(date.month()).ok()?,
        u32::try_from(date.day()).ok()?,
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OrderEventContext {
    pub correlation_id: Option<Uuid>,
    pub causation_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewOrder {
    pub channel: OrderChannel,
    pub fulfillment: Fulfillment,
    /// Resolved by the authorized channel adapter on the server.
    pub catalog_channel: String,
    pub catalog_version: i64,
    pub items: Vec<OrderItemSelection>,
    pub discount: Option<Discount>,
    pub delivery_fee_centavos: i64,
    pub acceptance_deadline_at: DateTime<Utc>,
    pub event_context: OrderEventContext,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderItemSelection {
    pub product_id: String,
    pub variation_id: Option<String>,
    pub quantity: i64,
    pub addons: Vec<OrderAddonSelection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderAddonSelection {
    pub option_id: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    Closed,
    ItemUnavailable,
    AddressNotServed,
    TooBusy,
    SuspectedFraud,
    Other,
    AcceptanceTimeout,
}

impl RejectReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::ItemUnavailable => "item_unavailable",
            Self::AddressNotServed => "address_not_served",
            Self::TooBusy => "too_busy",
            Self::SuspectedFraud => "suspected_fraud",
            Self::Other => "other",
            Self::AcceptanceTimeout => "acceptance_timeout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    ConsumerRequest,
    ItemUnavailable,
    DeliveryFailed,
    PaymentIssue,
    Other,
}

impl CancelReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConsumerRequest => "consumer_request",
            Self::ItemUnavailable => "item_unavailable",
            Self::DeliveryFailed => "delivery_failed",
            Self::PaymentIssue => "payment_issue",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OrderAction {
    Accept { promised_at: DateTime<Utc> },
    Reject { reason: RejectReason },
    Prepare,
    MarkReady,
    Complete,
    Cancel { reason: CancelReason },
}

impl OrderAction {
    pub const fn target(self) -> OrderState {
        match self {
            Self::Accept { .. } => OrderState::Accepted,
            Self::Reject { .. } => OrderState::Rejected,
            Self::Prepare => OrderState::Preparing,
            Self::MarkReady => OrderState::Ready,
            Self::Complete => OrderState::Completed,
            Self::Cancel { .. } => OrderState::Cancelled,
        }
    }

    pub const fn reason_code(self) -> Option<&'static str> {
        match self {
            Self::Reject { reason } => Some(reason.as_str()),
            Self::Cancel { reason } => Some(reason.as_str()),
            _ => None,
        }
    }

    pub const fn promised_at(self) -> Option<DateTime<Utc>> {
        match self {
            Self::Accept { promised_at } => Some(promised_at),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TransitionOrder<'a> {
    pub expected_state: OrderState,
    pub expected_version: i64,
    pub action: OrderAction,
    pub actor: Actor,
    pub actor_account_id: Option<&'a str>,
    pub event_context: OrderEventContext,
}
