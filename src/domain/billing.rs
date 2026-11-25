use chrono::{DateTime, FixedOffset, Utc};

/// Billing cycle of a SaaS subscription. Kept under its historical name for the
/// checkout, onboarding and Google sign-in flows (`?plan=monthly|semiannual|annual`).
/// Prices and discounts come exclusively from the commercial catalog.
pub use crate::domain::commercial::BillingCycle as BillingPlan;

pub const CHECKOUT_MINUTES: u32 = 30;

/// Undiscounted monthly price of the entry offer (QR Menu), from the commercial catalog.
pub fn entry_monthly_price_centavos() -> i64 {
    BillingPlan::Monthly.full_price()
}

pub fn billing_now() -> DateTime<FixedOffset> {
    let offset = FixedOffset::west_opt(3 * 3600).expect("valid billing UTC offset");
    Utc::now().with_timezone(&offset)
}
