use askama::Template;

use super::PageMeta;
use crate::domain::commerce::business_hours::CommercialSettings;
use crate::domain::delivery::coupon::Coupon;
use crate::domain::delivery::storefront::StorefrontSettings;
use crate::domain::delivery::zone::DeliveryZone;

#[derive(Template)]
#[template(path = "dashboard/delivery/settings.html")]
pub struct DeliverySettingsTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub csrf_token: String,
    pub is_admin: bool,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub active_nav: &'static str,
    pub slug: String,
    pub storefront_url: String,
    pub storefront: StorefrontSettings,
    pub commercial: CommercialSettings,
    pub zones: Vec<DeliveryZone>,
    pub coupons: Vec<Coupon>,
    pub can_edit: bool,
}

pub struct OrderCenterItemView {
    pub name: String,
    pub variation_name: Option<String>,
    pub quantity: i64,
    pub line_total_display: String,
    pub options_summary: String,
    pub note: String,
}

pub struct OrderCenterCardView {
    pub id: String,
    pub public_id: String,
    pub order_number: i64,
    pub business_date: String,
    pub channel: String,
    pub fulfillment: String,
    pub state: String,
    pub version: i64,
    pub total_display: String,
    pub discount_display: String,
    pub fee_display: String,
    pub neighborhood_label: String,
    pub address_summary: String,
    pub customer_phone: String,
    pub customer_name: String,
    pub change_for_display: Option<String>,
    pub coupon_code: Option<String>,
    pub order_note: String,
    pub payment_id: Option<String>,
    pub payment_version: Option<i64>,
    pub payment_method: Option<String>,
    pub payment_state: Option<String>,
    pub payment_confirmed_centavos: i64,
    pub payment_refunded_centavos: i64,
    pub payment_refund_due_centavos: i64,
    pub payment_pending_on_completed: bool,
    pub items: Vec<OrderCenterItemView>,
    pub can_accept: bool,
    pub can_reject: bool,
    pub can_prepare: bool,
    pub can_ready: bool,
    pub can_complete: bool,
    pub can_cancel: bool,
    pub can_confirm_payment: bool,
    pub can_refund: bool,
}

#[derive(Template)]
#[template(path = "dashboard/delivery/orders.html")]
pub struct DeliveryOrdersTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub csrf_token: String,
    pub is_admin: bool,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub active_nav: &'static str,
    pub state_filter: String,
    pub date_filter: String,
    pub can_settings: bool,
    pub push_key: Option<String>,
    pub push_scope: String,
    pub orders: Vec<OrderCenterCardView>,
}

#[derive(Template)]
#[template(path = "dashboard/delivery/order_list.html")]
pub struct DeliveryOrderListTemplate {
    pub page: PageMeta,
    pub csrf_token: String,
    pub orders: Vec<OrderCenterCardView>,
}
