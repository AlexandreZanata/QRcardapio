use chrono::{DateTime, FixedOffset, Months, NaiveDate, Utc};

pub const MONTHLY_PRICE_CENTAVOS: i64 = 2990;
pub const CHECKOUT_MINUTES: u32 = 30;

pub fn billing_now() -> DateTime<FixedOffset> {
    let offset = FixedOffset::west_opt(3 * 3600).expect("valid billing UTC offset");
    Utc::now().with_timezone(&offset)
}

pub fn paid_period_end(due_date: NaiveDate) -> Option<NaiveDate> {
    due_date.checked_add_months(Months::new(1))
}
