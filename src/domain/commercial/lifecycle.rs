use serde::Deserialize;

/// Availability state of a catalog offer. Behavior is documented in
/// `contracts/commercial/module-lifecycle.json`; tests enforce parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Planned,
    Development,
    InternalPreview,
    Beta,
    Available,
    Deprecated,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalePolicy {
    Never,
    ExplicitEnablement,
    Standard,
}

impl SalePolicy {
    /// Ordering from least to most sellable.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Never => 0,
            Self::ExplicitEnablement => 1,
            Self::Standard => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantPolicy {
    Never,
    ExplicitGrantOnly,
    Always,
}

impl Lifecycle {
    pub const ALL: [Self; 7] = [
        Self::Planned,
        Self::Development,
        Self::InternalPreview,
        Self::Beta,
        Self::Available,
        Self::Deprecated,
        Self::Retired,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Development => "development",
            Self::InternalPreview => "internal_preview",
            Self::Beta => "beta",
            Self::Available => "available",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == raw)
    }

    pub const fn sale(self) -> SalePolicy {
        match self {
            Self::Available => SalePolicy::Standard,
            Self::Beta => SalePolicy::ExplicitEnablement,
            Self::Planned
            | Self::Development
            | Self::InternalPreview
            | Self::Deprecated
            | Self::Retired => SalePolicy::Never,
        }
    }

    pub const fn grants(self) -> GrantPolicy {
        match self {
            Self::Beta | Self::Available | Self::Deprecated => GrantPolicy::Always,
            Self::InternalPreview => GrantPolicy::ExplicitGrantOnly,
            Self::Planned | Self::Development | Self::Retired => GrantPolicy::Never,
        }
    }

    pub const fn renewals(self) -> bool {
        matches!(self, Self::Beta | Self::Available | Self::Deprecated)
    }

    /// States in which the offer is or was sold; recorded versions become immutable.
    pub const fn ever_sellable(self) -> bool {
        matches!(
            self,
            Self::Beta | Self::Available | Self::Deprecated | Self::Retired
        )
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Planned, Self::Development)
                | (Self::Development, Self::Planned)
                | (Self::Development, Self::InternalPreview)
                | (Self::InternalPreview, Self::Development)
                | (Self::InternalPreview, Self::Beta)
                | (Self::Beta, Self::InternalPreview)
                | (Self::Beta, Self::Available)
                | (Self::Available, Self::Deprecated)
                | (Self::Deprecated, Self::Available)
                | (Self::Deprecated, Self::Retired)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_available_is_sold_normally_and_planned_never_grants() {
        assert_eq!(Lifecycle::Available.sale(), SalePolicy::Standard);
        assert_eq!(Lifecycle::Beta.sale(), SalePolicy::ExplicitEnablement);
        for state in [
            Lifecycle::Planned,
            Lifecycle::Development,
            Lifecycle::InternalPreview,
            Lifecycle::Deprecated,
            Lifecycle::Retired,
        ] {
            assert_eq!(state.sale(), SalePolicy::Never);
        }
        assert_eq!(Lifecycle::Planned.grants(), GrantPolicy::Never);
        assert_eq!(Lifecycle::Retired.grants(), GrantPolicy::Never);
        assert!(!Lifecycle::Available.can_transition_to(Lifecycle::Planned));
        assert!(Lifecycle::Beta.can_transition_to(Lifecycle::Available));
    }
}
