use std::sync::LazyLock;

use chrono::NaiveDate;
use serde::Deserialize;

use super::{BillingCycle, Lifecycle, Quote, QuoteRequest, SaleChannel};
use crate::domain::entitlements::Capability;

const MODULES_JSON: &str = include_str!("../../../contracts/commercial/modules.json");
const BUNDLES_JSON: &str = include_str!("../../../contracts/commercial/bundles.json");
const ADDONS_JSON: &str = include_str!("../../../contracts/commercial/addons.json");
const CYCLES_JSON: &str = include_str!("../../../contracts/commercial/billing-cycles.json");

/// Offer that every legacy "Plano Essencial" contract maps to and that the landing page sells.
pub const ENTRY_OFFER: &str = "menu";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OfferKind {
    Module,
    Bundle,
    Addon,
}

impl OfferKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Bundle => "bundle",
            Self::Addon => "addon",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Module, Self::Bundle, Self::Addon]
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityVersion {
    pub version: u32,
    pub effective_from: NaiveDate,
    pub monthly_price_centavos: i64,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleDef {
    pub code: String,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub phase: u8,
    pub affiliate_eligible: bool,
    pub requires_modules: Vec<String>,
    pub current_version: u32,
    pub versions: Vec<CapabilityVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleRef {
    pub code: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BundleVersion {
    pub version: u32,
    pub effective_from: NaiveDate,
    pub monthly_price_centavos: i64,
    pub modules: Vec<ModuleRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BundleDef {
    pub code: String,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub affiliate_eligible: bool,
    pub current_version: u32,
    pub versions: Vec<BundleVersion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonPricing {
    Recurring,
    OneTimeQuote,
    CustomQuote,
    UsageBased,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddonDef {
    pub code: String,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub pricing: AddonPricing,
    pub affiliate_eligible: bool,
    pub current_version: Option<u32>,
    pub versions: Vec<CapabilityVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CycleDef {
    pub code: String,
    pub months: i64,
    pub discount_bps: i64,
    pub gateway_cycle: String,
}

#[derive(Deserialize)]
struct ModulesFile {
    currency: String,
    modules: Vec<ModuleDef>,
}

#[derive(Deserialize)]
struct BundlesFile {
    bundles: Vec<BundleDef>,
}

#[derive(Deserialize)]
struct AddonsFile {
    addons: Vec<AddonDef>,
}

#[derive(Deserialize)]
struct CyclesFile {
    rounding: String,
    payment_grace_days: i64,
    cycles: Vec<CycleDef>,
}

/// The validated commercial catalog. Prices exist nowhere else in the codebase.
#[derive(Debug, Clone)]
pub struct CommercialCatalog {
    pub currency: String,
    pub rounding: String,
    pub payment_grace_days: i64,
    pub modules: Vec<ModuleDef>,
    pub bundles: Vec<BundleDef>,
    pub addons: Vec<AddonDef>,
    pub cycles: Vec<CycleDef>,
}

static CATALOG: LazyLock<CommercialCatalog> = LazyLock::new(|| {
    CommercialCatalog::load(MODULES_JSON, BUNDLES_JSON, ADDONS_JSON, CYCLES_JSON)
        .unwrap_or_else(|problems| panic!("invalid commercial catalog: {}", problems.join("; ")))
});

/// The compiled, validated commercial catalog.
pub fn catalog() -> &'static CommercialCatalog {
    &CATALOG
}

impl CommercialCatalog {
    /// Parses and validates the four catalog contracts. Returns every problem found.
    pub fn load(
        modules: &str,
        bundles: &str,
        addons: &str,
        cycles: &str,
    ) -> Result<Self, Vec<String>> {
        let parse_err = |name: &str, e: serde_json::Error| vec![format!("{name}: {e}")];
        let modules: ModulesFile =
            serde_json::from_str(modules).map_err(|e| parse_err("modules.json", e))?;
        let bundles: BundlesFile =
            serde_json::from_str(bundles).map_err(|e| parse_err("bundles.json", e))?;
        let addons: AddonsFile =
            serde_json::from_str(addons).map_err(|e| parse_err("addons.json", e))?;
        let cycles: CyclesFile =
            serde_json::from_str(cycles).map_err(|e| parse_err("billing-cycles.json", e))?;
        let catalog = Self {
            currency: modules.currency,
            rounding: cycles.rounding,
            payment_grace_days: cycles.payment_grace_days,
            modules: modules.modules,
            bundles: bundles.bundles,
            addons: addons.addons,
            cycles: cycles.cycles,
        };
        let problems = super::validation::validate(&catalog);
        if problems.is_empty() {
            Ok(catalog)
        } else {
            Err(problems)
        }
    }

    pub fn module(&self, code: &str) -> Option<&ModuleDef> {
        self.modules.iter().find(|m| m.code == code)
    }

    pub fn bundle(&self, code: &str) -> Option<&BundleDef> {
        self.bundles.iter().find(|b| b.code == code)
    }

    pub fn addon(&self, code: &str) -> Option<&AddonDef> {
        self.addons.iter().find(|a| a.code == code)
    }

    pub fn cycle(&self, cycle: BillingCycle) -> &CycleDef {
        self.cycles
            .iter()
            .find(|c| c.code == cycle.key())
            .expect("validated catalog defines every billing cycle")
    }

    /// Lifecycle of the module or add-on that owns a capability.
    pub fn capability_lifecycle(&self, capability: Capability) -> Lifecycle {
        let offer = capability.offer();
        self.module(offer)
            .map(|m| m.lifecycle)
            .or_else(|| self.addon(offer).map(|a| a.lifecycle))
            .expect("validated catalog assigns every capability to an offer")
    }

    /// Capabilities granted by the current version of a module or add-on.
    pub fn offer_capabilities(&self, code: &str) -> Vec<Capability> {
        self.module(code)
            .and_then(|m| m.version(m.current_version))
            .or_else(|| {
                self.addon(code)
                    .and_then(|a| a.current_version.and_then(|v| a.version(v)))
            })
            .map(|v| v.capabilities.clone())
            .unwrap_or_default()
    }

    /// Standard quote for the entry offer (QR Menu) on a cycle.
    pub fn entry_quote(&self, cycle: BillingCycle) -> Quote {
        self.quote(
            &QuoteRequest::module(ENTRY_OFFER, cycle),
            SaleChannel::Standard,
        )
        .expect("validated catalog keeps the entry offer sellable")
    }
}

impl ModuleDef {
    pub fn version(&self, version: u32) -> Option<&CapabilityVersion> {
        self.versions.iter().find(|v| v.version == version)
    }

    pub fn current(&self) -> &CapabilityVersion {
        self.version(self.current_version)
            .expect("validated module has its current version")
    }
}

impl BundleDef {
    pub fn version(&self, version: u32) -> Option<&BundleVersion> {
        self.versions.iter().find(|v| v.version == version)
    }
}

impl AddonDef {
    pub fn version(&self, version: u32) -> Option<&CapabilityVersion> {
        self.versions.iter().find(|v| v.version == version)
    }
}
