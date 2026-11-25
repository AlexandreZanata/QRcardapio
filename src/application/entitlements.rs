//! Entitlement Engine: resolves commercial capabilities and authorizes operations across the
//! five independent dimensions of ADR-008.

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::domain::commercial::{GrantPolicy, catalog};
use crate::domain::entitlements::{
    AccessSource, Capability, CoveragePeriod, EntitlementDecision, GrantKind, MemberRole,
    Operation, OperationalStatus, resolve,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::entitlements as queries;

/// Whether this deployment charges for commercial capabilities (technical feature flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    /// Billing configured: capabilities require a paid contract or an explicit grant.
    Enforced { environment: &'static str },
    /// No payment gateway configured (local preview, self-hosted): available modules are not
    /// commercially gated, preserving the behavior that predates the Commercial Engine.
    NotEnforced,
}

/// Why an operation was refused. Each dimension has its own user-facing message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denial {
    ModuleUnavailable,
    EstablishmentSuspended(OperationalStatus),
    PermissionDenied,
    NotEntitled(EntitlementDecision),
}

impl Denial {
    pub const fn i18n_key(&self) -> &'static str {
        match self {
            Self::ModuleUnavailable => "error.module_unavailable",
            Self::EstablishmentSuspended(_) => "error.establishment_suspended",
            Self::PermissionDenied => "error.permission_denied",
            Self::NotEntitled(_) => "error.subscription_required",
        }
    }

    pub fn into_error(self) -> AppError {
        AppError::Forbidden(self.i18n_key())
    }
}

/// Result of a successful authorization. `decision` is `None` for operations that need no
/// commercial capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    pub operation: Operation,
    pub decision: Option<EntitlementDecision>,
}

/// Every coverage period of one establishment, grouped later per capability.
pub struct EstablishmentCoverage {
    periods: Vec<(Capability, CoveragePeriod)>,
}

impl EstablishmentCoverage {
    /// Loads paid contract periods of `environment` (none when billing is not enforced) and
    /// every explicit grant of the establishment.
    pub async fn load(
        pool: &SqlitePool,
        environment: Option<&str>,
        establishment_id: &str,
    ) -> AppResult<Self> {
        let mut periods = Vec::new();
        for row in queries::coverage_rows(pool, environment, establishment_id).await? {
            let renewing = row.renewing;
            let (starts_on, ends_on) = (row.starts_on, row.ends_on);
            match (row.source.as_str(), row.capability) {
                ("contract", Some(raw)) => {
                    let Some(capability) = Capability::parse(&raw) else {
                        tracing::warn!(capability = %raw, "contract grants an unknown capability");
                        continue;
                    };
                    let source = AccessSource::Contract {
                        contract_id: row.source_id,
                    };
                    periods.push((
                        capability,
                        CoveragePeriod {
                            source,
                            starts_on,
                            ends_on,
                            renewing,
                        },
                    ));
                }
                ("grant", _) => {
                    let Some(kind) = row.grant_kind.as_deref().and_then(GrantKind::parse) else {
                        continue;
                    };
                    for capability in catalog().offer_capabilities(&row.offer_code) {
                        let source = AccessSource::Grant {
                            grant_id: row.source_id.clone(),
                            kind,
                        };
                        periods.push((
                            capability,
                            CoveragePeriod {
                                source,
                                starts_on,
                                ends_on,
                                renewing: false,
                            },
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(Self { periods })
    }

    /// Resolves a capability, honoring the lifecycle of the module that owns it.
    pub fn decide(&self, capability: Capability, today: NaiveDate) -> EntitlementDecision {
        let policy = catalog().capability_lifecycle(capability).grants();
        let periods: Vec<CoveragePeriod> = self
            .periods
            .iter()
            .filter(|(cap, period)| {
                *cap == capability
                    && match policy {
                        GrantPolicy::Always => true,
                        GrantPolicy::ExplicitGrantOnly => matches!(
                            period.source,
                            AccessSource::Grant {
                                kind: GrantKind::InternalPreview,
                                ..
                            }
                        ),
                        GrantPolicy::Never => false,
                    }
            })
            .map(|(_, period)| period.clone())
            .collect();
        resolve(&periods, today, catalog().payment_grace_days)
    }
}

/// Resolves one capability for an establishment under the deployment's enforcement mode.
pub async fn resolve_capability(
    pool: &SqlitePool,
    enforcement: Enforcement,
    establishment_id: &str,
    capability: Capability,
    today: NaiveDate,
) -> AppResult<EntitlementDecision> {
    let environment = match (
        catalog().capability_lifecycle(capability).grants(),
        enforcement,
    ) {
        (GrantPolicy::Never, _) => return Ok(resolve(&[], today, 0)),
        (GrantPolicy::Always, Enforcement::NotEnforced) => {
            return Ok(EntitlementDecision::not_enforced());
        }
        // Internal previews always need an explicit grant, even without billing.
        (GrantPolicy::ExplicitGrantOnly, Enforcement::NotEnforced) => None,
        (_, Enforcement::Enforced { environment }) => Some(environment),
    };
    let coverage = EstablishmentCoverage::load(pool, environment, establishment_id).await?;
    Ok(coverage.decide(capability, today))
}

/// Authorizes an operation. Every applicable dimension must pass, evaluated in the order:
/// module lifecycle, establishment operational state, member permission, deployment
/// enforcement and commercial entitlement.
pub async fn authorize(
    pool: &SqlitePool,
    enforcement: Enforcement,
    establishment_id: &str,
    role: MemberRole,
    operation: Operation,
    today: NaiveDate,
) -> AppResult<Result<Authorization, Denial>> {
    let capability = operation.capability();
    if let Some(capability) = capability
        && catalog().capability_lifecycle(capability).grants() == GrantPolicy::Never
    {
        return Ok(Err(Denial::ModuleUnavailable));
    }
    let status = queries::operational_status(pool, establishment_id).await?;
    let status = OperationalStatus::parse(&status).unwrap_or(OperationalStatus::Active);
    if status.blocks(operation) {
        return Ok(Err(Denial::EstablishmentSuspended(status)));
    }
    if !operation.permits(role) {
        return Ok(Err(Denial::PermissionDenied));
    }
    let Some(capability) = capability else {
        return Ok(Ok(Authorization {
            operation,
            decision: None,
        }));
    };
    let decision =
        resolve_capability(pool, enforcement, establishment_id, capability, today).await?;
    if decision.is_allowed() {
        Ok(Ok(Authorization {
            operation,
            decision: Some(decision),
        }))
    } else {
        Ok(Err(Denial::NotEntitled(decision)))
    }
}

/// Whether the establishment currently holds `menu.publish` (display and MCP read models).
pub async fn can_publish_menu(
    pool: &SqlitePool,
    enforcement: Enforcement,
    establishment_id: &str,
) -> AppResult<bool> {
    let decision = resolve_capability(
        pool,
        enforcement,
        establishment_id,
        Capability::MenuPublish,
        crate::domain::billing::billing_now().date_naive(),
    )
    .await?;
    Ok(decision.is_allowed())
}
