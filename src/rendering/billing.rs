use askama::Template;
use chrono::NaiveDate;

use super::PageMeta;
use crate::domain::billing::MONTHLY_PRICE_CENTAVOS;
use crate::i18n::{Locale, format_price};

#[derive(Template)]
#[template(path = "dashboard/billing.html")]
pub struct BillingTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub plan_price: String,
    pub paid_through: Option<NaiveDate>,
    pub status_key: &'static str,
    pub is_owner: bool,
    pub start_checkout: bool,
    pub checkout_url: Option<String>,
    pub can_reconcile: bool,
    pub cancellable_id: Option<String>,
}

pub fn plan_price() -> String {
    format_price(MONTHLY_PRICE_CENTAVOS, Locale::PtBr)
}
