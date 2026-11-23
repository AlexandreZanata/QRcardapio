use askama::Template;
use chrono::NaiveDate;

use super::PageMeta;
use crate::domain::billing::BillingPlan;
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
    pub plan: PlanOffer,
    pub plans: [PlanOffer; 3],
    pub paid_through: Option<NaiveDate>,
    pub status_key: &'static str,
    pub is_owner: bool,
    pub start_checkout: bool,
    pub checkout_url: Option<String>,
    pub can_reconcile: bool,
    pub cancellable_id: Option<String>,
}

pub struct PlanOffer {
    pub key: &'static str,
    pub name_key: &'static str,
    pub period_key: &'static str,
    pub price: String,
    pub full_price: String,
    pub discount: i64,
    pub art: &'static str,
    pub recommended: bool,
}

impl PlanOffer {
    pub fn new(plan: BillingPlan, price_centavos: i64) -> Self {
        let (name_key, period_key, art) = match plan {
            BillingPlan::Monthly => ("billing.monthly", "billing.per_month", "plan-monthly"),
            BillingPlan::Semiannual => (
                "billing.semiannual",
                "billing.per_semester",
                "plan-semiannual",
            ),
            BillingPlan::Annual => ("billing.annual", "billing.per_year", "plan-annual"),
        };
        Self {
            key: plan.key(),
            name_key,
            period_key,
            price: format_price(price_centavos, Locale::PtBr),
            full_price: format_price(plan.full_price(), Locale::PtBr),
            discount: plan.discount(),
            art,
            recommended: plan == BillingPlan::Annual,
        }
    }

    pub fn catalog() -> [Self; 3] {
        BillingPlan::ALL.map(|plan| Self::new(plan, plan.price()))
    }
}
