use std::collections::BTreeSet;

use super::{
    AddonPricing, BillingCycle, CommercialCatalog, Lifecycle, OfferKind, SalePolicy, apply_discount,
};
use crate::domain::entitlements::Capability;

/// Reference to a catalog offer requested by a customer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferRef {
    pub kind: OfferKind,
    pub code: String,
}

/// Offers and cycle requested for one consolidated subscription.
#[derive(Debug, Clone)]
pub struct QuoteRequest {
    pub offers: Vec<OfferRef>,
    pub cycle: BillingCycle,
}

impl QuoteRequest {
    pub fn module(code: &str, cycle: BillingCycle) -> Self {
        Self {
            offers: vec![OfferRef {
                kind: OfferKind::Module,
                code: code.to_owned(),
            }],
            cycle,
        }
    }
}

/// Who is buying: the standard self-service flow or an explicitly enabled beta sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleChannel {
    Standard,
    BetaEnablement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteError {
    Empty,
    UnknownOffer,
    NotSellable,
    NotSelfService,
    Duplicate,
    Overlap,
}

impl QuoteError {
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Empty | Self::UnknownOffer => "error.commercial_offer_invalid",
            Self::NotSellable | Self::NotSelfService => "error.commercial_offer_unavailable",
            Self::Duplicate | Self::Overlap => "error.commercial_offer_overlap",
        }
    }
}

/// A capability granted by a quote line, tagged with the offer that grants it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineGrant {
    pub offer_code: String,
    pub capability: Capability,
}

/// Priced, versioned line. Everything a contract snapshot needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteLine {
    pub kind: OfferKind,
    pub code: String,
    pub name: String,
    pub version: u32,
    pub monthly_price_centavos: i64,
    pub list_price_centavos: i64,
    pub discount_bps: i64,
    pub price_centavos: i64,
    pub affiliate_eligible: bool,
    pub grants: Vec<LineGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub cycle: BillingCycle,
    pub currency: String,
    pub lines: Vec<QuoteLine>,
    pub list_total_centavos: i64,
    pub total_centavos: i64,
    pub affiliate_eligible_centavos: i64,
}

impl Quote {
    /// Distinct capabilities granted by the whole quote.
    pub fn capabilities(&self) -> BTreeSet<Capability> {
        self.lines
            .iter()
            .flat_map(|line| line.grants.iter().map(|g| g.capability))
            .collect()
    }

    pub fn offer_names(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.name.as_str())
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

struct Resolved {
    name: String,
    lifecycle: Lifecycle,
    version: u32,
    monthly: i64,
    affiliate_eligible: bool,
    grants: Vec<LineGrant>,
    covers_modules: Vec<String>,
}

impl CommercialCatalog {
    /// Prices a consolidated subscription. Only the backend computes prices; the browser
    /// submits offer codes and a cycle, never amounts.
    pub fn quote(&self, request: &QuoteRequest, channel: SaleChannel) -> Result<Quote, QuoteError> {
        if request.offers.is_empty() {
            return Err(QuoteError::Empty);
        }
        let cycle = self.cycle(request.cycle);
        let mut covered: BTreeSet<String> = BTreeSet::new();
        let mut seen: BTreeSet<(OfferKind, String)> = BTreeSet::new();
        let mut lines = Vec::with_capacity(request.offers.len());
        for offer in &request.offers {
            if !seen.insert((offer.kind, offer.code.clone())) {
                return Err(QuoteError::Duplicate);
            }
            let resolved = self.resolve_offer(offer)?;
            let sellable = match resolved.lifecycle.sale() {
                SalePolicy::Standard => true,
                SalePolicy::ExplicitEnablement => channel == SaleChannel::BetaEnablement,
                SalePolicy::Never => false,
            };
            if !sellable {
                return Err(QuoteError::NotSellable);
            }
            for module in &resolved.covers_modules {
                if !covered.insert(module.clone()) {
                    return Err(QuoteError::Overlap);
                }
            }
            let list = resolved.monthly * request.cycle.months();
            lines.push(QuoteLine {
                kind: offer.kind,
                code: offer.code.clone(),
                name: resolved.name,
                version: resolved.version,
                monthly_price_centavos: resolved.monthly,
                list_price_centavos: list,
                discount_bps: cycle.discount_bps,
                price_centavos: apply_discount(list, cycle.discount_bps),
                affiliate_eligible: resolved.affiliate_eligible,
                grants: resolved.grants,
            });
        }
        let list_total_centavos = lines.iter().map(|l| l.list_price_centavos).sum();
        let total_centavos = lines.iter().map(|l| l.price_centavos).sum();
        let affiliate_eligible_centavos = lines
            .iter()
            .filter(|l| l.affiliate_eligible)
            .map(|l| l.price_centavos)
            .sum();
        Ok(Quote {
            cycle: request.cycle,
            currency: self.currency.clone(),
            lines,
            list_total_centavos,
            total_centavos,
            affiliate_eligible_centavos,
        })
    }

    fn resolve_offer(&self, offer: &OfferRef) -> Result<Resolved, QuoteError> {
        match offer.kind {
            OfferKind::Module => {
                let module = self.module(&offer.code).ok_or(QuoteError::UnknownOffer)?;
                let version = module.current();
                Ok(Resolved {
                    name: module.name.clone(),
                    lifecycle: module.lifecycle,
                    version: version.version,
                    monthly: version.monthly_price_centavos,
                    affiliate_eligible: module.affiliate_eligible,
                    grants: grants_of(&module.code, &version.capabilities),
                    covers_modules: vec![module.code.clone()],
                })
            }
            OfferKind::Bundle => {
                let bundle = self.bundle(&offer.code).ok_or(QuoteError::UnknownOffer)?;
                let version = bundle
                    .version(bundle.current_version)
                    .ok_or(QuoteError::UnknownOffer)?;
                let mut grants = Vec::new();
                let mut lifecycle = bundle.lifecycle;
                for pinned in &version.modules {
                    let module = self.module(&pinned.code).ok_or(QuoteError::UnknownOffer)?;
                    let module_version = module
                        .version(pinned.version)
                        .ok_or(QuoteError::UnknownOffer)?;
                    // A bundle is never more sellable than its least available module.
                    if module.lifecycle.sale().rank() < lifecycle.sale().rank() {
                        lifecycle = module.lifecycle;
                    }
                    grants.extend(grants_of(&module.code, &module_version.capabilities));
                }
                Ok(Resolved {
                    name: bundle.name.clone(),
                    lifecycle,
                    version: version.version,
                    monthly: version.monthly_price_centavos,
                    affiliate_eligible: bundle.affiliate_eligible,
                    grants,
                    covers_modules: version.modules.iter().map(|m| m.code.clone()).collect(),
                })
            }
            OfferKind::Addon => {
                let addon = self.addon(&offer.code).ok_or(QuoteError::UnknownOffer)?;
                if addon.pricing != AddonPricing::Recurring {
                    return Err(QuoteError::NotSelfService);
                }
                let version = addon
                    .current_version
                    .and_then(|v| addon.version(v))
                    .ok_or(QuoteError::NotSelfService)?;
                Ok(Resolved {
                    name: addon.name.clone(),
                    lifecycle: addon.lifecycle,
                    version: version.version,
                    monthly: version.monthly_price_centavos,
                    affiliate_eligible: addon.affiliate_eligible,
                    grants: grants_of(&addon.code, &version.capabilities),
                    covers_modules: Vec::new(),
                })
            }
        }
    }
}

fn grants_of(offer_code: &str, capabilities: &[Capability]) -> Vec<LineGrant> {
    capabilities
        .iter()
        .map(|&capability| LineGrant {
            offer_code: offer_code.to_owned(),
            capability,
        })
        .collect()
}
