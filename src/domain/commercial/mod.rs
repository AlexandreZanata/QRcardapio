//! Commercial Engine domain: the versioned catalog compiled from `contracts/commercial/`,
//! lifecycle rules, billing cycles, integer pricing and quotes. Pure code; no I/O.

mod catalog;
mod cycle;
mod fingerprint;
mod lifecycle;
mod quote;
mod validation;

pub use catalog::{
    AddonDef, AddonPricing, BundleDef, BundleVersion, CapabilityVersion, CommercialCatalog,
    CycleDef, ENTRY_OFFER, ModuleDef, ModuleRef, OfferKind, catalog,
};
pub use cycle::{BillingCycle, apply_discount};
pub use fingerprint::{OfferFingerprint, VersionFingerprint};
pub use lifecycle::{GrantPolicy, Lifecycle, SalePolicy};
pub use quote::{LineGrant, OfferRef, Quote, QuoteError, QuoteLine, QuoteRequest, SaleChannel};
