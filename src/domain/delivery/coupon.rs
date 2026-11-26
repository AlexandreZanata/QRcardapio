use chrono::{DateTime, NaiveDateTime, Utc};

use crate::domain::commerce::business_hours::{ServiceMode, zone};
use crate::domain::commerce::discount::Discount;
use crate::domain::commerce::pricing::MAX_ORDER_TOTAL_CENTAVOS;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CouponKind {
    PercentBps,
    AmountCentavos,
}

impl CouponKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PercentBps => "percent_bps",
            Self::AmountCentavos => "amount_centavos",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "percent_bps" => Some(Self::PercentBps),
            "amount_centavos" => Some(Self::AmountCentavos),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coupon {
    pub id: String,
    pub establishment_id: String,
    pub code: String,
    pub kind: CouponKind,
    pub percent_bps: Option<i64>,
    pub amount_centavos: Option<i64>,
    pub min_subtotal_centavos: i64,
    pub valid_from: Option<NaiveDateTime>,
    pub valid_until: Option<NaiveDateTime>,
    pub max_redemptions: Option<i64>,
    pub max_per_customer: Option<i64>,
    pub redemption_count: i64,
    pub allow_delivery: bool,
    pub allow_pickup: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CouponEvaluation {
    Applied(Discount),
    Rejected,
}

impl Coupon {
    /// Evaluates eligibility using one generic `Rejected` result to prevent enumeration hints.
    pub fn evaluate(
        &self,
        mode: ServiceMode,
        items_subtotal_centavos: i64,
        now: DateTime<Utc>,
        time_zone: &str,
        customer_redemptions: i64,
    ) -> CouponEvaluation {
        if !self.is_active || items_subtotal_centavos < self.min_subtotal_centavos {
            return CouponEvaluation::Rejected;
        }
        let mode_allowed = match mode {
            ServiceMode::Delivery => self.allow_delivery,
            ServiceMode::Pickup => self.allow_pickup,
        };
        if !mode_allowed {
            return CouponEvaluation::Rejected;
        }
        if let Some(max) = self.max_redemptions
            && self.redemption_count >= max
        {
            return CouponEvaluation::Rejected;
        }
        if let Some(max_customer) = self.max_per_customer
            && customer_redemptions >= max_customer
        {
            return CouponEvaluation::Rejected;
        }
        if let Some(local_now) = local_naive(now, time_zone) {
            if self.valid_from.is_some_and(|start| local_now < start) {
                return CouponEvaluation::Rejected;
            }
            if self.valid_until.is_some_and(|end| local_now > end) {
                return CouponEvaluation::Rejected;
            }
        } else if self.valid_from.is_some() || self.valid_until.is_some() {
            return CouponEvaluation::Rejected;
        }
        let discount = match (self.kind, self.percent_bps, self.amount_centavos) {
            (CouponKind::PercentBps, Some(bps), None) => Discount::BasisPoints(bps),
            (CouponKind::AmountCentavos, None, Some(amount)) => Discount::FixedCentavos(amount),
            _ => return CouponEvaluation::Rejected,
        };
        CouponEvaluation::Applied(discount)
    }
}

fn local_naive(now: DateTime<Utc>, time_zone: &str) -> Option<NaiveDateTime> {
    let tz = zone(time_zone)?;
    let ts = jiff::Timestamp::new(
        now.timestamp(),
        i32::try_from(now.timestamp_subsec_nanos()).ok()?,
    )
    .ok()?;
    let dt = ts.to_zoned_datetime(tz).datetime();
    let date = chrono::NaiveDate::from_ymd_opt(
        i32::from(dt.year()),
        u32::try_from(dt.month()).ok()?,
        u32::try_from(dt.day()).ok()?,
    )?;
    let time = chrono::NaiveTime::from_hms_opt(
        u32::try_from(dt.hour()).ok()?,
        u32::try_from(dt.minute()).ok()?,
        u32::try_from(dt.second()).ok()?,
    )?;
    Some(NaiveDateTime::new(date, time))
}

pub fn validate_coupon_code(raw: &str) -> AppResult<String> {
    let code = raw.trim().to_ascii_uppercase();
    if !(3..=20).contains(&code.len())
        || !code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(AppError::Validation("delivery.invalid_coupon"));
    }
    Ok(code)
}

pub struct CouponInput<'a> {
    pub code: &'a str,
    pub kind: CouponKind,
    pub value: i64,
    pub min_subtotal_centavos: i64,
    pub valid_from: Option<NaiveDateTime>,
    pub valid_until: Option<NaiveDateTime>,
    pub max_redemptions: Option<i64>,
    pub max_per_customer: Option<i64>,
    pub allow_delivery: bool,
    pub allow_pickup: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedCoupon {
    pub code: String,
    pub kind: CouponKind,
    pub percent_bps: Option<i64>,
    pub amount_centavos: Option<i64>,
    pub min_subtotal_centavos: i64,
    pub valid_from: Option<NaiveDateTime>,
    pub valid_until: Option<NaiveDateTime>,
    pub max_redemptions: Option<i64>,
    pub max_per_customer: Option<i64>,
    pub allow_delivery: bool,
    pub allow_pickup: bool,
    pub is_active: bool,
}

impl ValidatedCoupon {
    pub fn validate(input: CouponInput<'_>) -> AppResult<Self> {
        const INVALID: AppError = AppError::Validation("delivery.invalid_coupon");
        let code = validate_coupon_code(input.code)?;
        let (percent_bps, amount_centavos) = match input.kind {
            CouponKind::PercentBps => {
                if !(1..=10_000).contains(&input.value) {
                    return Err(INVALID);
                }
                (Some(input.value), None)
            }
            CouponKind::AmountCentavos => {
                if !(1..=MAX_ORDER_TOTAL_CENTAVOS).contains(&input.value) {
                    return Err(INVALID);
                }
                (None, Some(input.value))
            }
        };
        if !(0..=MAX_ORDER_TOTAL_CENTAVOS).contains(&input.min_subtotal_centavos) {
            return Err(INVALID);
        }
        if let (Some(start), Some(end)) = (input.valid_from, input.valid_until)
            && end < start
        {
            return Err(INVALID);
        }
        if input.max_redemptions.is_some_and(|v| v < 1)
            || input.max_per_customer.is_some_and(|v| v < 1)
            || (!input.allow_delivery && !input.allow_pickup)
        {
            return Err(INVALID);
        }
        Ok(Self {
            code,
            kind: input.kind,
            percent_bps,
            amount_centavos,
            min_subtotal_centavos: input.min_subtotal_centavos,
            valid_from: input.valid_from,
            valid_until: input.valid_until,
            max_redemptions: input.max_redemptions,
            max_per_customer: input.max_per_customer,
            allow_delivery: input.allow_delivery,
            allow_pickup: input.allow_pickup,
            is_active: input.is_active,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupon_validation_and_generic_rejection_rules() {
        assert_eq!(validate_coupon_code(" bem10 ").unwrap(), "BEM10");
        assert!(validate_coupon_code("AB").is_err());
        assert!(validate_coupon_code("BEM-10").is_err());

        let now = "2026-11-26T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let coupon = Coupon {
            id: "c1".into(),
            establishment_id: "e1".into(),
            code: "BEM10".into(),
            kind: CouponKind::PercentBps,
            percent_bps: Some(1000),
            amount_centavos: None,
            min_subtotal_centavos: 3000,
            valid_from: Some(
                NaiveDateTime::parse_from_str("2026-11-26T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap(),
            ),
            valid_until: Some(
                NaiveDateTime::parse_from_str("2026-11-26T22:00:00", "%Y-%m-%dT%H:%M:%S").unwrap(),
            ),
            max_redemptions: Some(5),
            max_per_customer: Some(1),
            redemption_count: 1,
            allow_delivery: true,
            allow_pickup: false,
            is_active: true,
        };
        assert_eq!(
            coupon.evaluate(ServiceMode::Delivery, 3000, now, "America/Sao_Paulo", 0),
            CouponEvaluation::Applied(Discount::BasisPoints(1000))
        );
        assert_eq!(
            coupon.evaluate(ServiceMode::Pickup, 3000, now, "America/Sao_Paulo", 0),
            CouponEvaluation::Rejected
        );
        assert_eq!(
            coupon.evaluate(ServiceMode::Delivery, 2999, now, "America/Sao_Paulo", 0),
            CouponEvaluation::Rejected
        );
        assert_eq!(
            coupon.evaluate(ServiceMode::Delivery, 3000, now, "America/Sao_Paulo", 1),
            CouponEvaluation::Rejected
        );
    }
}
