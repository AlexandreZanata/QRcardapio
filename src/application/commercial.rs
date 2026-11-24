//! Commercial Engine use cases: backend-only quotes and the module overview shown to owners.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::application::entitlements::{Enforcement, EstablishmentCoverage};
use crate::domain::commercial::{
    BillingCycle, GrantPolicy, Lifecycle, OfferKind, OfferRef, Quote, QuoteRequest, SaleChannel,
    SalePolicy, catalog,
};
use crate::domain::entitlements::{EntitlementDecision, EntitlementStatus};
use crate::error::{AppError, AppResult};

/// Prices a self-service checkout. The browser only sends offer codes and a cycle.
pub fn quote_checkout(offers: &[OfferRef], cycle: BillingCycle) -> AppResult<Quote> {
    let request = QuoteRequest {
        offers: offers.to_vec(),
        cycle,
    };
    catalog()
        .quote(&request, SaleChannel::Standard)
        .map_err(|error| AppError::Validation(error.i18n_key()))
}

/// Parses a submitted offer code (`menu`, `bundle:start`, `addon:custom_domain`).
pub fn parse_offer(raw: &str) -> AppResult<OfferRef> {
    let (kind, code) = match raw.split_once(':') {
        Some((kind, code)) => (
            OfferKind::parse(kind).ok_or(AppError::Validation("error.commercial_offer_invalid"))?,
            code,
        ),
        None => (OfferKind::Module, raw),
    };
    if code.is_empty() || code.len() > 40 {
        return Err(AppError::Validation("error.commercial_offer_invalid"));
    }
    Ok(OfferRef {
        kind,
        code: code.to_owned(),
    })
}

/// How a module appears to an owner. Planned modules are informational only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleStanding {
    Contracted,
    Available,
    Expired,
    Suspended,
    InDevelopment,
}

impl ModuleStanding {
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Contracted => "commercial.status_contracted",
            Self::Available => "commercial.status_available",
            Self::Expired => "commercial.status_expired",
            Self::Suspended => "commercial.status_suspended",
            Self::InDevelopment => "commercial.status_in_development",
        }
    }

    pub const fn css(self) -> &'static str {
        match self {
            Self::Contracted => "contracted",
            Self::Available => "available",
            Self::Expired => "expired",
            Self::Suspended => "suspended",
            Self::InDevelopment => "development",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleOverview {
    pub code: String,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub standing: ModuleStanding,
    /// Monthly price is shown only for sellable modules; planned prices are not announced.
    pub monthly_price_centavos: Option<i64>,
    pub decision: Option<EntitlementDecision>,
}

/// Lists every catalog module with its commercial standing for one establishment.
pub async fn module_overview(
    pool: &SqlitePool,
    enforcement: Enforcement,
    establishment_id: &str,
    suspended: bool,
    today: NaiveDate,
) -> AppResult<Vec<ModuleOverview>> {
    let coverage = match enforcement {
        Enforcement::Enforced { environment } => {
            Some(EstablishmentCoverage::load(pool, Some(environment), establishment_id).await?)
        }
        Enforcement::NotEnforced => None,
    };
    let mut modules: Vec<_> = catalog().modules.iter().collect();
    modules.sort_by_key(|module| (module.phase, module.code.as_str()));
    Ok(modules
        .into_iter()
        .map(|module| {
            let visible = module.lifecycle.grants() == GrantPolicy::Always;
            let for_sale = module.lifecycle.sale() == SalePolicy::Standard;
            // Every capability of a module comes from the same contract line or grant, so the
            // first one represents the module.
            let decision = match (&coverage, visible) {
                (Some(coverage), true) => {
                    Some(coverage.decide(module.current().capabilities[0], today))
                }
                _ => None,
            };
            let status = decision.as_ref().map(|d| d.status);
            let standing = match (visible, suspended, status) {
                (false, _, _) => ModuleStanding::InDevelopment,
                (true, true, _) => ModuleStanding::Suspended,
                (true, false, Some(EntitlementStatus::Active | EntitlementStatus::Grace)) => {
                    ModuleStanding::Contracted
                }
                (true, false, Some(EntitlementStatus::Expired)) => ModuleStanding::Expired,
                _ if for_sale => ModuleStanding::Available,
                _ => ModuleStanding::InDevelopment,
            };
            ModuleOverview {
                code: module.code.clone(),
                name: module.name.clone(),
                lifecycle: module.lifecycle,
                standing,
                monthly_price_centavos: for_sale.then(|| module.current().monthly_price_centavos),
                decision,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_offer_codes_without_trusting_prices() {
        assert_eq!(parse_offer("menu").unwrap().kind, OfferKind::Module);
        assert_eq!(parse_offer("bundle:start").unwrap().kind, OfferKind::Bundle);
        assert!(parse_offer("plan:premium").is_err());
        assert!(parse_offer("").is_err());
    }

    #[test]
    fn only_available_modules_can_be_quoted() {
        let menu = quote_checkout(&[parse_offer("menu").unwrap()], BillingCycle::Annual).unwrap();
        assert_eq!(menu.total_centavos, 32_292);
        for planned in [
            "delivery",
            "bundle:start",
            "bundle:complete",
            "addon:custom_domain",
        ] {
            let error = quote_checkout(&[parse_offer(planned).unwrap()], BillingCycle::Monthly)
                .unwrap_err();
            assert_eq!(error.i18n_key(), "error.commercial_offer_unavailable");
        }
        let quoted = quote_checkout(
            &[parse_offer("addon:assisted_onboarding").unwrap()],
            BillingCycle::Monthly,
        )
        .unwrap_err();
        assert_eq!(quoted.i18n_key(), "error.commercial_offer_unavailable");
    }
}
