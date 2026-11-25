use chrono::{Duration, NaiveDate};

use crate::domain::commercial::Lifecycle;

/// Kinds of explicit, operator-issued grants (`entitlement_grants.grant_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    Trial,
    Beta,
    InternalPreview,
    Courtesy,
}

impl GrantKind {
    pub const ALL: [Self; 4] = [
        Self::Trial,
        Self::Beta,
        Self::InternalPreview,
        Self::Courtesy,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trial => "trial",
            Self::Beta => "beta",
            Self::InternalPreview => "internal_preview",
            Self::Courtesy => "courtesy",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    /// Lifecycle states in which the grant may be issued.
    pub const fn allowed_for(self, lifecycle: Lifecycle) -> bool {
        matches!(
            (self, lifecycle),
            (Self::Trial, Lifecycle::Available)
                | (Self::Beta, Lifecycle::Beta)
                | (
                    Self::InternalPreview,
                    Lifecycle::InternalPreview | Lifecycle::Beta
                )
                | (Self::Courtesy, Lifecycle::Available | Lifecycle::Deprecated)
        )
    }

    pub const fn max_days(self) -> i64 {
        match self {
            Self::Trial => 31,
            Self::Beta | Self::InternalPreview | Self::Courtesy => 366,
        }
    }
}

/// Where an active capability comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessSource {
    Contract {
        contract_id: String,
    },
    Grant {
        grant_id: String,
        kind: GrantKind,
    },
    /// Billing is not enforced by this deployment (no payment gateway configured).
    NotEnforced,
}

/// One period during which a source covers a capability. `ends_on` is exclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoveragePeriod {
    pub source: AccessSource,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    /// True when the source is a contract whose gateway subscription still renews.
    pub renewing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitlementStatus {
    Active,
    /// Renewal payment is late but within the published payment grace window.
    Grace,
    /// Covered in the future only.
    Scheduled,
    Expired,
    NotContracted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitlementDecision {
    pub status: EntitlementStatus,
    pub source: Option<AccessSource>,
    pub starts_on: Option<NaiveDate>,
    pub ends_on: Option<NaiveDate>,
    /// Usage limits are not metered yet; every current capability is unlimited.
    pub usage_limit: Option<i64>,
}

impl EntitlementDecision {
    pub const fn is_allowed(&self) -> bool {
        matches!(
            self.status,
            EntitlementStatus::Active | EntitlementStatus::Grace
        )
    }

    pub fn not_enforced() -> Self {
        Self {
            status: EntitlementStatus::Active,
            source: Some(AccessSource::NotEnforced),
            starts_on: None,
            ends_on: None,
            usage_limit: None,
        }
    }

    fn of(status: EntitlementStatus, period: Option<&CoveragePeriod>) -> Self {
        Self {
            status,
            source: period.map(|p| p.source.clone()),
            starts_on: period.map(|p| p.starts_on),
            ends_on: period.map(|p| p.ends_on),
            usage_limit: None,
        }
    }
}

/// Resolves one capability from every coverage period of every source.
///
/// Multiple sources never cancel each other: the capability is active while any period
/// covers `today`, and the reported source is the one that lasts longest.
pub fn resolve(
    periods: &[CoveragePeriod],
    today: NaiveDate,
    grace_days: i64,
) -> EntitlementDecision {
    let active = periods
        .iter()
        .filter(|p| p.starts_on <= today && today < p.ends_on)
        .max_by_key(|p| (p.ends_on, matches!(p.source, AccessSource::Contract { .. })));
    if let Some(period) = active {
        return EntitlementDecision::of(EntitlementStatus::Active, Some(period));
    }
    let last_ended = periods
        .iter()
        .filter(|p| p.ends_on <= today)
        .max_by_key(|p| p.ends_on);
    if let Some(period) = last_ended
        && period.renewing
        && grace_days > 0
        && today < period.ends_on + Duration::days(grace_days)
    {
        return EntitlementDecision::of(EntitlementStatus::Grace, Some(period));
    }
    if let Some(next) = periods
        .iter()
        .filter(|p| p.starts_on > today)
        .min_by_key(|p| p.starts_on)
    {
        return EntitlementDecision::of(EntitlementStatus::Scheduled, Some(next));
    }
    match last_ended {
        Some(period) => EntitlementDecision::of(EntitlementStatus::Expired, Some(period)),
        None => EntitlementDecision::of(EntitlementStatus::NotContracted, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(raw: &str) -> NaiveDate {
        raw.parse().unwrap()
    }

    fn contract(id: &str, from: &str, to: &str, renewing: bool) -> CoveragePeriod {
        CoveragePeriod {
            source: AccessSource::Contract {
                contract_id: id.into(),
            },
            starts_on: day(from),
            ends_on: day(to),
            renewing,
        }
    }

    fn grant(id: &str, from: &str, to: &str) -> CoveragePeriod {
        CoveragePeriod {
            source: AccessSource::Grant {
                grant_id: id.into(),
                kind: GrantKind::Courtesy,
            },
            starts_on: day(from),
            ends_on: day(to),
            renewing: false,
        }
    }

    #[test]
    fn end_date_is_exclusive_and_expiry_is_reported() {
        let periods = [contract("k1", "2026-01-31", "2026-02-28", true)];
        let active = resolve(&periods, day("2026-02-27"), 0);
        assert_eq!(active.status, EntitlementStatus::Active);
        assert_eq!(active.ends_on, Some(day("2026-02-28")));
        let expired = resolve(&periods, day("2026-02-28"), 0);
        assert_eq!(expired.status, EntitlementStatus::Expired);
        assert!(!expired.is_allowed());
    }

    #[test]
    fn grace_applies_only_to_renewing_contracts() {
        let renewing = [contract("k1", "2026-01-01", "2026-02-01", true)];
        assert_eq!(
            resolve(&renewing, day("2026-02-03"), 3).status,
            EntitlementStatus::Grace
        );
        assert_eq!(
            resolve(&renewing, day("2026-02-04"), 3).status,
            EntitlementStatus::Expired
        );
        let cancelled = [contract("k1", "2026-01-01", "2026-02-01", false)];
        assert_eq!(
            resolve(&cancelled, day("2026-02-02"), 3).status,
            EntitlementStatus::Expired
        );
        let granted = [grant("g1", "2026-01-01", "2026-02-01")];
        assert_eq!(
            resolve(&granted, day("2026-02-02"), 3).status,
            EntitlementStatus::Expired
        );
    }

    #[test]
    fn overlapping_sources_keep_access_and_report_the_longest() {
        let periods = [
            contract("k1", "2026-01-01", "2026-02-01", false),
            grant("g1", "2026-01-15", "2026-03-01"),
        ];
        let decision = resolve(&periods, day("2026-01-20"), 0);
        assert_eq!(decision.status, EntitlementStatus::Active);
        assert_eq!(
            decision.source,
            Some(AccessSource::Grant {
                grant_id: "g1".into(),
                kind: GrantKind::Courtesy
            })
        );
        // Removing the contract source leaves the grant in force.
        let remaining = [grant("g1", "2026-01-15", "2026-03-01")];
        assert!(resolve(&remaining, day("2026-02-10"), 0).is_allowed());
    }

    #[test]
    fn future_and_missing_coverage_are_denied() {
        let scheduled = [grant("g1", "2026-05-01", "2026-06-01")];
        let decision = resolve(&scheduled, day("2026-04-30"), 0);
        assert_eq!(decision.status, EntitlementStatus::Scheduled);
        assert!(!decision.is_allowed());
        assert_eq!(
            resolve(&[], day("2026-04-30"), 0).status,
            EntitlementStatus::NotContracted
        );
    }

    #[test]
    fn grants_respect_lifecycle() {
        assert!(GrantKind::Trial.allowed_for(Lifecycle::Available));
        assert!(!GrantKind::Trial.allowed_for(Lifecycle::Planned));
        assert!(GrantKind::Beta.allowed_for(Lifecycle::Beta));
        assert!(!GrantKind::Beta.allowed_for(Lifecycle::Available));
        assert!(!GrantKind::Courtesy.allowed_for(Lifecycle::Retired));
        for kind in GrantKind::ALL {
            assert_eq!(GrantKind::parse(kind.as_str()), Some(kind));
            assert!(!kind.allowed_for(Lifecycle::Planned));
            assert!(!kind.allowed_for(Lifecycle::Development));
        }
    }
}
