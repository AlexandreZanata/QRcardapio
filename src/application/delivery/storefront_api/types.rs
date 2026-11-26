use serde::{Deserialize, Serialize};

use crate::domain::commerce::pricing::OrderTotal;

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontOptionInput {
    pub option_id: String,
    pub quantity: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontLineInput {
    pub product_id: String,
    pub variation_id: Option<String>,
    pub quantity: i64,
    #[serde(default)]
    pub options: Vec<StorefrontOptionInput>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StorefrontQuoteInput {
    pub catalog_version: i64,
    pub fulfillment: String,
    pub zone_id: Option<String>,
    pub coupon_code: Option<String>,
    pub lines: Vec<StorefrontLineInput>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontCustomerInput {
    pub name: String,
    pub phone: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontAddressInput {
    pub street: String,
    pub number: String,
    #[serde(default)]
    pub complement: String,
    pub neighborhood_zone_id: Option<String>,
    pub city: String,
    pub uf: String,
    pub cep: Option<String>,
    #[serde(default)]
    pub reference: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontPaymentInput {
    pub method: String,
    pub change_for_centavos: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontExpectedTotals {
    pub items_subtotal_centavos: i64,
    pub discount_centavos: i64,
    pub delivery_fee_centavos: i64,
    pub total_centavos: i64,
}

impl From<StorefrontExpectedTotals> for OrderTotal {
    fn from(total: StorefrontExpectedTotals) -> Self {
        Self {
            items_subtotal_centavos: total.items_subtotal_centavos,
            discount_centavos: total.discount_centavos,
            delivery_fee_centavos: total.delivery_fee_centavos,
            total_centavos: total.total_centavos,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StorefrontOrderInput {
    pub catalog_version: i64,
    pub fulfillment: String,
    pub zone_id: Option<String>,
    pub coupon_code: Option<String>,
    pub lines: Vec<StorefrontLineInput>,
    pub expected_total_centavos: i64,
    pub expected_totals: StorefrontExpectedTotals,
    pub customer: StorefrontCustomerInput,
    pub address: Option<StorefrontAddressInput>,
    pub payment: StorefrontPaymentInput,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StorefrontQuoteLine {
    pub unit_price_centavos: Option<i64>,
    pub line_total_centavos: Option<i64>,
    pub available: bool,
}

#[derive(Debug, Serialize)]
pub struct StorefrontCouponResult {
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct StorefrontQuoteResponse {
    pub catalog_version: i64,
    pub lines: Vec<StorefrontQuoteLine>,
    pub items_subtotal_centavos: Option<i64>,
    pub discount_centavos: Option<i64>,
    pub delivery_fee_centavos: Option<i64>,
    pub total_centavos: Option<i64>,
    pub minimum_order_centavos: i64,
    pub coupon: Option<StorefrontCouponResult>,
    pub open_now: bool,
    pub eta_minutes: Option<[i64; 2]>,
    pub issues: Vec<String>,
}

#[derive(Serialize)]
pub struct StorefrontOrderReceipt {
    pub order_id: String,
    pub tracking_path: String,
    pub cancel_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pix_key_display: Option<String>,
}

pub enum StorefrontOrderOutcome {
    Placed(StorefrontOrderReceipt),
    Repriced(StorefrontQuoteResponse),
}

#[derive(Debug, Serialize)]
pub struct TrackingItemOption {
    pub group_name: String,
    pub group_name_en: String,
    pub group_name_es: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub unit_price_centavos: i64,
    pub quantity: i64,
}

#[derive(Debug, Serialize)]
pub struct TrackingItem {
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub variation_name: Option<String>,
    pub variation_name_en: Option<String>,
    pub variation_name_es: Option<String>,
    pub unit_price_centavos: i64,
    pub quantity: i64,
    pub line_total_centavos: i64,
    pub options: Vec<TrackingItemOption>,
}

#[derive(Debug, Serialize)]
pub struct TrackingPayment {
    pub method: String,
    pub state: String,
    pub amount_centavos: i64,
    pub change_for_centavos: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TrackingTotals {
    pub items_subtotal_centavos: i64,
    pub discount_centavos: i64,
    pub delivery_fee_centavos: i64,
    pub total_centavos: i64,
}

#[derive(Debug, Serialize)]
pub struct StorefrontTrackingView {
    pub order_id: String,
    pub order_number: i64,
    pub channel: String,
    pub fulfillment: String,
    pub state: String,
    pub version: i64,
    pub can_cancel: bool,
    pub placed_at: String,
    pub accepted_at: Option<String>,
    pub promised_at: Option<String>,
    pub preparing_at: Option<String>,
    pub ready_at: Option<String>,
    pub completed_at: Option<String>,
    pub rejected_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub reason_code: Option<String>,
    pub neighborhood_label: String,
    pub contact_whatsapp: String,
    pub totals: TrackingTotals,
    pub payment: Option<TrackingPayment>,
    pub items: Vec<TrackingItem>,
}
