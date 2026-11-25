use chrono::{Datelike, Duration, NaiveDate};

use crate::domain::affiliate::{
    CommissionType, MAX_COOKIE_TTL_DAYS, MAX_HOLD_DAYS, MAX_PERCENT_BPS, MAX_PIX_KEY_LEN,
    MIN_COOKIE_TTL_DAYS, MIN_HOLD_DAYS, max_fixed_monthly_centavos,
};
use crate::domain::product::parse_price_centavos;
use crate::error::{AppError, AppResult};
use crate::i18n::{Locale, format_price};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledInstallment {
    pub installment_index: i64,
    pub commission_centavos: i64,
    pub available_on: NaiveDate,
}

/// Validates a commission rule pair `(commission_type, commission_value)`.
pub fn validate_commission_value(kind: CommissionType, value: i64) -> AppResult<i64> {
    match kind {
        CommissionType::FixedMonthlyCentavos => {
            if (0..=max_fixed_monthly_centavos()).contains(&value) {
                Ok(value)
            } else {
                Err(AppError::Validation("error.affiliate_commission_invalid"))
            }
        }
        CommissionType::PercentBps => {
            if (0..=MAX_PERCENT_BPS).contains(&value) {
                Ok(value)
            } else {
                Err(AppError::Validation("error.affiliate_commission_invalid"))
            }
        }
    }
}

/// Parses human-entered commission value into integer centavos or basis points (no floats).
pub fn parse_commission_input(kind: CommissionType, raw: &str) -> AppResult<i64> {
    let value = match kind {
        CommissionType::FixedMonthlyCentavos => parse_price_centavos(raw)
            .map_err(|_| AppError::Validation("error.affiliate_commission_invalid"))?,
        CommissionType::PercentBps => parse_percent_bps(raw)?,
    };
    validate_commission_value(kind, value)
}

/// Parses a percentage string like `"20"`, `"12,5"`, `"12.50%"` into basis points (`0..=10000`).
pub fn parse_percent_bps(raw: &str) -> AppResult<i64> {
    let cleaned = raw.trim().trim_end_matches('%').trim();
    if cleaned.is_empty() || cleaned.starts_with('-') {
        return Err(AppError::Validation("error.affiliate_commission_invalid"));
    }
    let normalized = cleaned.replace(',', ".");
    let mut parts = normalized.split('.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next();
    if parts.next().is_some() || int_part.is_empty() {
        return Err(AppError::Validation("error.affiliate_commission_invalid"));
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::Validation("error.affiliate_commission_invalid"));
    }
    let whole: i64 = int_part
        .parse()
        .map_err(|_| AppError::Validation("error.affiliate_commission_invalid"))?;
    let frac_bps: i64 = match frac_part {
        None | Some("") => 0,
        Some(f) if f.len() == 1 && f.bytes().all(|b| b.is_ascii_digit()) => {
            let d: i64 = f
                .parse()
                .map_err(|_| AppError::Validation("error.affiliate_commission_invalid"))?;
            d * 10
        }
        Some(f) if f.len() == 2 && f.bytes().all(|b| b.is_ascii_digit()) => f
            .parse()
            .map_err(|_| AppError::Validation("error.affiliate_commission_invalid"))?,
        _ => return Err(AppError::Validation("error.affiliate_commission_invalid")),
    };
    let bps = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac_bps))
        .ok_or(AppError::Validation("error.affiliate_commission_invalid"))?;
    validate_commission_value(CommissionType::PercentBps, bps)
}

pub fn format_percent_bps(bps: i64, locale: Locale) -> String {
    let whole = bps / 100;
    let frac = bps % 100;
    let sep = match locale {
        Locale::PtBr | Locale::Es => ',',
        Locale::En => '.',
    };
    if frac == 0 {
        format!("{whole}%")
    } else if frac % 10 == 0 {
        format!("{whole}{sep}{}%", frac / 10)
    } else {
        format!("{whole}{sep}{frac:02}%")
    }
}

pub fn format_commission_input_value(kind: CommissionType, value: i64) -> String {
    match kind {
        CommissionType::FixedMonthlyCentavos => format!("{},{:02}", value / 100, value % 100),
        CommissionType::PercentBps => {
            let whole = value / 100;
            let frac = value % 100;
            if frac == 0 {
                whole.to_string()
            } else if frac % 10 == 0 {
                format!("{whole},{}", frac / 10)
            } else {
                format!("{whole},{frac:02}")
            }
        }
    }
}

pub fn format_commission_rate(kind: CommissionType, value: i64, locale: Locale) -> String {
    match kind {
        CommissionType::FixedMonthlyCentavos => format_price(value, locale),
        CommissionType::PercentBps => format_percent_bps(value, locale),
    }
}

pub fn validate_hold_days(days: i64) -> AppResult<i64> {
    if (MIN_HOLD_DAYS..=MAX_HOLD_DAYS).contains(&days) {
        Ok(days)
    } else {
        Err(AppError::Validation("error.affiliate_hold_days_invalid"))
    }
}

pub fn validate_cookie_ttl_days(days: i64) -> AppResult<i64> {
    if (MIN_COOKIE_TTL_DAYS..=MAX_COOKIE_TTL_DAYS).contains(&days) {
        Ok(days)
    } else {
        Err(AppError::Validation("error.affiliate_cookie_ttl_invalid"))
    }
}

pub fn validate_pix_key(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > MAX_PIX_KEY_LEN || trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation("error.affiliate_pix_invalid"));
    }
    Ok(trimmed.to_owned())
}

pub fn validate_optional_text(raw: &str, max_len: usize) -> String {
    raw.trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max_len)
        .collect()
}

/// Canonicalizes an email address to detect simple self-referral bypasses
/// (`+alias` subaddressing and Gmail dot/googlemail equivalence).
pub fn canonical_email_identity(email: &str) -> String {
    let lower = email.trim().to_ascii_lowercase();
    let Some((local, domain)) = lower.split_once('@') else {
        return lower;
    };
    let domain = if domain == "googlemail.com" {
        "gmail.com"
    } else {
        domain
    };
    let base_local = local.split('+').next().unwrap_or(local);
    let cleaned_local = if domain == "gmail.com" {
        base_local.replace('.', "")
    } else {
        base_local.to_owned()
    };
    format!("{cleaned_local}@{domain}")
}

/// Masks a customer email for the affiliate portal (LGPD-friendly privacy).
pub fn mask_email_for_partner(email: &str) -> String {
    let Some((local, domain)) = email.split_once('@') else {
        return "***".to_owned();
    };
    let visible: String = local.chars().take(2).collect();
    format!("{visible}***@{domain}")
}

/// Splits a confirmed subscription payment into month-by-month commission installments
/// using pure integer centavos math with exact remainder distribution (zero rounding drift).
pub fn compute_installment_schedule(
    commission_type: CommissionType,
    commission_value: i64,
    plan_months: i64,
    confirmed_centavos: i64,
    due_date: NaiveDate,
    hold_days: i64,
) -> Vec<ScheduledInstallment> {
    if !matches!(plan_months, 1 | 6 | 12) || confirmed_centavos <= 0 || commission_value <= 0 {
        return Vec::new();
    }
    let total_centavos = match commission_type {
        CommissionType::FixedMonthlyCentavos => commission_value
            .saturating_mul(plan_months)
            .clamp(0, confirmed_centavos),
        CommissionType::PercentBps => {
            let numer = (confirmed_centavos as i128)
                .saturating_mul(commission_value as i128)
                .saturating_add(5_000);
            ((numer / 10_000) as i64).clamp(0, confirmed_centavos)
        }
    };
    if total_centavos <= 0 {
        return Vec::new();
    }
    let base = total_centavos / plan_months;
    let remainder = total_centavos % plan_months;
    let hold = Duration::days(hold_days.clamp(MIN_HOLD_DAYS, MAX_HOLD_DAYS));

    let mut schedule = Vec::with_capacity(plan_months as usize);
    for idx in 1..=plan_months {
        let amount = base + i64::from(idx <= remainder);
        if amount <= 0 {
            continue;
        }
        let period_start = add_months_floor(due_date, (idx - 1) as u32);
        schedule.push(ScheduledInstallment {
            installment_index: idx,
            commission_centavos: amount,
            available_on: period_start + hold,
        });
    }
    schedule
}

/// Adds calendar months clamping day to the target month's last day (matches SQLite `'floor'`).
pub fn add_months_floor(date: NaiveDate, months: u32) -> NaiveDate {
    let total_months = date.year() * 12 + (date.month0() as i32) + (months as i32);
    let year = total_months.div_euclid(12);
    let month0 = total_months.rem_euclid(12) as u32;
    let month = month0 + 1;
    let last_day = days_in_month(year, month);
    let day = date.day().min(last_day);
    NaiveDate::from_ymd_opt(year, month, day).expect("valid floored calendar date")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap { 29 } else { 28 }
        }
        _ => 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_percent_bps_without_floating_point() {
        assert_eq!(parse_percent_bps("20").unwrap(), 2000);
        assert_eq!(parse_percent_bps("12,5%").unwrap(), 1250);
        assert_eq!(parse_percent_bps("12.35").unwrap(), 1235);
        assert_eq!(parse_percent_bps("0").unwrap(), 0);
        assert_eq!(parse_percent_bps("100").unwrap(), 10_000);
        assert!(parse_percent_bps("100.01").is_err());
        assert!(parse_percent_bps("-5").is_err());
        assert!(parse_percent_bps("12.345").is_err());
    }

    #[test]
    fn canonical_email_detects_plus_aliases_and_gmail_dots() {
        assert_eq!(
            canonical_email_identity("J.o.a.o+promo@googlemail.com"),
            canonical_email_identity("joao@gmail.com")
        );
        assert_eq!(
            canonical_email_identity("partner+test@empresa.com.br"),
            canonical_email_identity("partner@empresa.com.br")
        );
        assert_ne!(
            canonical_email_identity("a.b@empresa.com.br"),
            canonical_email_identity("ab@empresa.com.br")
        );
    }

    #[test]
    fn monthly_and_annual_fixed_schedules_never_exceed_payment() {
        let due = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        let annual = compute_installment_schedule(
            CommissionType::FixedMonthlyCentavos,
            800,
            12,
            32_292,
            due,
            15,
        );
        assert_eq!(annual.len(), 12);
        assert_eq!(
            annual.iter().map(|i| i.commission_centavos).sum::<i64>(),
            9_600
        );
        assert_eq!(
            annual[0].available_on,
            NaiveDate::from_ymd_opt(2026, 2, 15).unwrap()
        );
        assert_eq!(
            annual[1].available_on,
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
        );

        let capped = compute_installment_schedule(
            CommissionType::FixedMonthlyCentavos,
            2_990,
            12,
            32_292,
            due,
            0,
        );
        assert_eq!(
            capped.iter().map(|i| i.commission_centavos).sum::<i64>(),
            32_292
        );
    }

    #[test]
    fn semiannual_percent_distributes_exact_centavos() {
        let due = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let sched =
            compute_installment_schedule(CommissionType::PercentBps, 2000, 6, 17_043, due, 15);
        assert_eq!(sched.len(), 6);
        assert_eq!(sched[0].commission_centavos, 569);
        assert_eq!(sched[1].commission_centavos, 568);
        assert_eq!(
            sched.iter().map(|i| i.commission_centavos).sum::<i64>(),
            3_409
        );
    }

    #[test]
    fn masks_emails_for_partner_privacy() {
        assert_eq!(
            mask_email_for_partner("maria.clara@padaria.com.br"),
            "ma***@padaria.com.br"
        );
        assert_eq!(mask_email_for_partner("a@b.com"), "a***@b.com");
    }
}
