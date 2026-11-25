use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::order::OrderEventContext;
use super::state_machine::{Actor, OrderPaymentState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineMethod {
    Cash,
    CardOnDelivery,
    PixOnDelivery,
}

impl OfflineMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::CardOnDelivery => "card_on_delivery",
            Self::PixOnDelivery => "pix_on_delivery",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "cash" => Self::Cash,
            "card_on_delivery" => Self::CardOnDelivery,
            "pix_on_delivery" => Self::PixOnDelivery,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflinePaymentChoice {
    pub method: OfflineMethod,
    pub change_for_centavos: Option<i64>,
}

impl OfflinePaymentChoice {
    pub fn accepts_change(self, order_total_centavos: i64) -> bool {
        self.change_for_centavos.is_none_or(|change| {
            self.method == OfflineMethod::Cash && change >= order_total_centavos
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderPayment {
    pub id: String,
    pub public_id: String,
    pub establishment_id: String,
    pub order_id: String,
    pub requested_method: OfflineMethod,
    pub method: OfflineMethod,
    pub state: OrderPaymentState,
    pub version: i64,
    pub amount_centavos: i64,
    pub refunded_centavos: i64,
    pub change_for_centavos: Option<i64>,
    pub refund_due_centavos: i64,
    pub refund_due_at: Option<DateTime<Utc>>,
    pub refund_due_event_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundReason {
    OrderCancelled,
    CustomerRequest,
    PaymentError,
    Other,
}

impl RefundReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OrderCancelled => "order_cancelled",
            Self::CustomerRequest => "customer_request",
            Self::PaymentError => "payment_error",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PaymentOperation<'a> {
    pub establishment_id: &'a str,
    pub order_id: &'a str,
    pub payment_id: &'a str,
    pub expected_version: i64,
    pub actor: Actor,
    pub actor_account_id: &'a str,
    pub event_context: OrderEventContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRefund {
    pub id: String,
    pub payment_id: String,
    pub amount_centavos: i64,
    pub reason: RefundReason,
    pub recorded_at: DateTime<Utc>,
}
