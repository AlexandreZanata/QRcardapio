//! Entitlement Engine domain: capabilities, access resolution and authorization rules.
//! Pure types only; persistence lives in `infrastructure::database::queries::entitlements`
//! and orchestration in `application::entitlements`.

mod access;
mod authorization;
mod capability;

pub use access::{
    AccessSource, CoveragePeriod, EntitlementDecision, EntitlementStatus, GrantKind, resolve,
};
pub use authorization::{AfterExpiry, MemberRole, Operation, OperationalStatus};
pub use capability::Capability;
