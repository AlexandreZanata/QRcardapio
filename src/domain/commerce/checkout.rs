use serde::Serialize;
use uuid::Uuid;

use super::business_hours::ServiceMode;
use super::customer::{CustomerProfile, PostalAddress};
use super::discount::Discount;
use super::order::OrderItemSelection;
use super::order_payment::OfflinePaymentChoice;
use super::pricing::{LinePrice, OrderTotal};

pub const REPRICED_CODE: &str = "commerce.checkout_repriced";

/// The channel adapter authorizes the operation and resolves these values server-side.
/// A client body supplies selections and the expected total through `CheckoutRequest`.
#[derive(Clone, Copy)]
pub struct CheckoutTarget<'a> {
    pub establishment_id: &'a str,
    pub catalog_channel: &'a str,
    pub mode: ServiceMode,
    pub delivery_fee_centavos: i64,
    pub discount: Option<Discount>,
}

#[derive(Clone, Serialize)]
pub struct CheckoutCart {
    pub catalog_version: i64,
    pub items: Vec<OrderItemSelection>,
}

pub struct CheckoutRequest {
    pub idempotency_key: Uuid,
    pub cart: CheckoutCart,
    pub expected_total_centavos: i64,
    pub customer: CustomerProfile,
    pub address: Option<PostalAddress>,
    pub payment: OfflinePaymentChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckoutQuote {
    pub catalog_version: i64,
    /// One entry per input line. `None` identifies an unavailable selection.
    pub lines: Vec<Option<LinePrice>>,
    /// A payable total exists only when all intended selections are available.
    pub totals: Option<OrderTotal>,
    pub issue: Option<CheckoutQuoteIssue>,
    pub minimum_order_centavos: i64,
    pub open_now: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutQuoteIssue {
    OrderLimit,
}

impl CheckoutQuoteIssue {
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::OrderLimit => "commerce.invalid_order_price",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckoutReceipt {
    pub public_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutOutcome {
    Placed(CheckoutReceipt),
    Repriced(CheckoutQuote),
}
