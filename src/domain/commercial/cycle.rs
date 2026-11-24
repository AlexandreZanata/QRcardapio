/// Billing cycles supported by the platform. Months are structural (the database
/// constrains them); discounts and gateway codes come from
/// `contracts/commercial/billing-cycles.json` through the commercial catalog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum BillingCycle {
    #[default]
    Monthly,
    Semiannual,
    Annual,
}

impl BillingCycle {
    pub const ALL: [Self; 3] = [Self::Monthly, Self::Semiannual, Self::Annual];

    pub fn from_key(raw: &str) -> Option<Self> {
        match raw {
            "1" | "monthly" => Some(Self::Monthly),
            "semiannual" => Some(Self::Semiannual),
            "annual" => Some(Self::Annual),
            _ => None,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Semiannual => "semiannual",
            Self::Annual => "annual",
        }
    }

    pub const fn months(self) -> i64 {
        match self {
            Self::Monthly => 1,
            Self::Semiannual => 6,
            Self::Annual => 12,
        }
    }

    pub fn from_months(months: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|cycle| cycle.months() == months)
    }

    /// Discount in basis points from the commercial catalog (`500` = 5%).
    pub fn discount_bps(self) -> i64 {
        super::catalog().cycle(self).discount_bps
    }

    /// Whole-percent discount for display (`5` for 5%).
    pub fn discount(self) -> i64 {
        self.discount_bps() / 100
    }

    /// Undiscounted cycle price of the entry offer (QR Menu), in centavos.
    pub fn full_price(self) -> i64 {
        super::catalog().entry_quote(self).list_total_centavos
    }

    /// Discounted cycle price of the entry offer (QR Menu), in centavos.
    pub fn price(self) -> i64 {
        super::catalog().entry_quote(self).total_centavos
    }
}

/// Applies a discount in basis points with deterministic round-half-up on integer centavos.
/// Every surface (backend, database snapshot, gateway payload, UI) uses this function.
pub fn apply_discount(list_centavos: i64, discount_bps: i64) -> i64 {
    debug_assert!((0..=10_000).contains(&discount_bps));
    debug_assert!(list_centavos >= 0);
    let numerator = i128::from(list_centavos) * i128::from(10_000 - discount_bps) + 5_000;
    i64::try_from(numerator / 10_000).expect("discounted price fits in i64")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discounts_round_half_up_on_integer_centavos() {
        assert_eq!(apply_discount(17_940, 500), 17_043);
        assert_eq!(apply_discount(35_880, 1_000), 32_292);
        assert_eq!(apply_discount(2_990, 0), 2_990);
        // 0.5 centavo rounds up, 0.4999 rounds down.
        assert_eq!(apply_discount(10, 5_000), 5);
        assert_eq!(apply_discount(1, 5_000), 1);
        assert_eq!(apply_discount(3, 5_000), 2);
        assert_eq!(apply_discount(83_940, 500), 79_743);
    }

    #[test]
    fn cycle_keys_round_trip() {
        for cycle in BillingCycle::ALL {
            assert_eq!(BillingCycle::from_key(cycle.key()), Some(cycle));
            assert_eq!(BillingCycle::from_months(cycle.months()), Some(cycle));
        }
        assert_eq!(BillingCycle::from_key("1"), Some(BillingCycle::Monthly));
        assert_eq!(BillingCycle::from_key("weekly"), None);
    }
}
