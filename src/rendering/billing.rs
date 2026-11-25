use askama::Template;
use chrono::NaiveDate;

use super::PageMeta;
use crate::application::commercial::{ModuleOverview, ModuleStanding};
use crate::domain::billing::BillingPlan;
use crate::domain::commercial::{ENTRY_OFFER, catalog};
use crate::domain::payments::PaymentMethod;
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
    pub modules: Vec<ModuleCard>,
    pub methods: Vec<MethodOffer>,
    pub inter_charge: Option<ChargeView>,
    pub show_payer_form: bool,
    pub payer_summary: Option<String>,
}

/// A payment method the owner can choose. `blocked_key` explains why it cannot start yet.
pub struct MethodOffer {
    pub key: &'static str,
    pub name_key: &'static str,
    pub hint_key: &'static str,
    pub submit_key: &'static str,
    pub blocked_key: &'static str,
}

impl MethodOffer {
    pub fn new(method: PaymentMethod, payer_on_file: bool) -> Self {
        let (hint_key, submit_key) = match method {
            PaymentMethod::Card => ("billing.method_card_hint", "billing.subscribe_card"),
            PaymentMethod::PixBoleto => (
                "billing.method_pix_boleto_hint",
                "billing.subscribe_pix_boleto",
            ),
        };
        Self {
            key: method.as_str(),
            name_key: method.i18n_key(),
            hint_key,
            submit_key,
            blocked_key: if method == PaymentMethod::PixBoleto && !payer_on_file {
                "error.payer_required"
            } else {
                ""
            },
        }
    }
}

/// Current Inter charge shown to the owner.
pub struct ChargeView {
    pub amount: String,
    pub due_date: NaiveDate,
    pub pix_copy_paste: Option<String>,
    pub digitable_line: Option<String>,
    pub status_key: &'static str,
}

/// One catalog module on the subscription page. Planned modules have no price or action.
pub struct ModuleCard {
    pub name: String,
    pub description_key: String,
    pub status_key: &'static str,
    pub badge: &'static str,
    pub price: Option<String>,
    pub active_until: Option<NaiveDate>,
    pub is_entry: bool,
}

impl ModuleCard {
    pub fn from_overview(module: ModuleOverview) -> Self {
        let badge = match module.standing {
            ModuleStanding::Contracted => "badge-success",
            ModuleStanding::Expired => "badge-warn",
            ModuleStanding::Suspended => "badge-danger",
            ModuleStanding::Available | ModuleStanding::InDevelopment => "badge-neutral",
        };
        let active_until = match module.standing {
            ModuleStanding::Contracted => module.decision.as_ref().and_then(|d| d.ends_on),
            _ => None,
        };
        Self {
            description_key: format!("module.{}.description", module.code),
            is_entry: module.code == ENTRY_OFFER,
            name: module.name,
            status_key: module.standing.i18n_key(),
            badge,
            price: module
                .monthly_price_centavos
                .map(|price| format_price(price, Locale::PtBr)),
            active_until,
        }
    }
}

pub struct PlanOffer {
    pub key: &'static str,
    pub offer: &'static str,
    pub offer_name: String,
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
        let entry = catalog()
            .module(ENTRY_OFFER)
            .expect("validated catalog defines the entry offer");
        Self {
            key: plan.key(),
            offer: ENTRY_OFFER,
            offer_name: entry.name.clone(),
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
