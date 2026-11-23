use chrono::{DateTime, FixedOffset, Utc};

pub const MONTHLY_PRICE_CENTAVOS: i64 = 2990;
pub const CHECKOUT_MINUTES: u32 = 30;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum BillingPlan {
    #[default]
    Monthly,
    Semiannual,
    Annual,
}

impl BillingPlan {
    pub const ALL: [Self; 3] = [Self::Monthly, Self::Semiannual, Self::Annual];

    pub fn from_key(raw: &str) -> Option<Self> {
        match raw {
            "1" | "monthly" => Some(Self::Monthly),
            "semiannual" => Some(Self::Semiannual),
            "annual" => Some(Self::Annual),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Semiannual => "semiannual",
            Self::Annual => "annual",
        }
    }

    pub fn months(self) -> i64 {
        match self {
            Self::Monthly => 1,
            Self::Semiannual => 6,
            Self::Annual => 12,
        }
    }

    pub fn from_months(months: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|plan| plan.months() == months)
    }

    pub fn discount(self) -> i64 {
        match self {
            Self::Monthly => 0,
            Self::Semiannual => 5,
            Self::Annual => 10,
        }
    }

    pub fn full_price(self) -> i64 {
        MONTHLY_PRICE_CENTAVOS * self.months()
    }

    pub fn price(self) -> i64 {
        (self.full_price() * (100 - self.discount()) + 50) / 100
    }
}

pub fn billing_now() -> DateTime<FixedOffset> {
    let offset = FixedOffset::west_opt(3 * 3600).expect("valid billing UTC offset");
    Utc::now().with_timezone(&offset)
}
