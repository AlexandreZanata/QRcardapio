use crate::domain::billing::MONTHLY_PRICE_CENTAVOS;
use crate::error::{AppError, AppResult};

pub use super::affiliate_math::*;

pub const MAX_FIXED_MONTHLY_CENTAVOS: i64 = MONTHLY_PRICE_CENTAVOS;
pub const MAX_PERCENT_BPS: i64 = 10_000;
pub const MIN_HOLD_DAYS: i64 = 0;
pub const MAX_HOLD_DAYS: i64 = 90;
pub const MIN_COOKIE_TTL_DAYS: i64 = 1;
pub const MAX_COOKIE_TTL_DAYS: i64 = 180;
pub const MAX_PIX_KEY_LEN: usize = 140;
pub const MAX_NOTES_LEN: usize = 400;
pub const MAX_PAYOUT_REF_LEN: usize = 120;

const RESERVED_CODES: &[&str] = &[
    "account",
    "admin",
    "affiliate",
    "affiliates",
    "afiliado",
    "afiliados",
    "api",
    "app",
    "auth",
    "billing",
    "c",
    "dashboard",
    "en",
    "es",
    "ferramentas",
    "guias",
    "healthz",
    "login",
    "logout",
    "mcp",
    "onboarding",
    "qr",
    "qrcardapio",
    "r",
    "readyz",
    "register",
    "reset-password",
    "robots",
    "sitemap",
    "solucoes",
    "static",
    "suporte",
    "support",
    "webhooks",
    "www",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommissionType {
    FixedMonthlyCentavos,
    PercentBps,
}

impl CommissionType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedMonthlyCentavos => "fixed_monthly_centavos",
            Self::PercentBps => "percent_bps",
        }
    }

    pub fn parse(raw: &str) -> AppResult<Self> {
        match raw.trim() {
            "fixed_monthly_centavos" => Ok(Self::FixedMonthlyCentavos),
            "percent_bps" => Ok(Self::PercentBps),
            _ => Err(AppError::Validation("error.affiliate_commission_invalid")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffiliateStatus {
    Active,
    Paused,
    Revoked,
}

impl AffiliateStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Revoked => "revoked",
        }
    }

    pub fn parse(raw: &str) -> AppResult<Self> {
        match raw.trim() {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "revoked" => Ok(Self::Revoked),
            _ => Err(AppError::Validation("error.affiliate_status_invalid")),
        }
    }

    pub const fn accepts_new_referrals(self) -> bool {
        matches!(self, Self::Active)
    }

    pub const fn accrues_commissions(self) -> bool {
        matches!(self, Self::Active | Self::Paused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferralStatus {
    SignedUp,
    Active,
    Disqualified,
}

impl ReferralStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SignedUp => "signed_up",
            Self::Active => "active",
            Self::Disqualified => "disqualified",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "active" => Self::Active,
            "disqualified" => Self::Disqualified,
            _ => Self::SignedUp,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommissionStatus {
    Pending,
    Available,
    Paid,
    Voided,
}

impl CommissionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Available => "available",
            Self::Paid => "paid",
            Self::Voided => "voided",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "available" => Self::Available,
            "paid" => Self::Paid,
            "voided" => Self::Voided,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Commission,
    Clawback,
}

impl EntryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Commission => "commission",
            Self::Clawback => "clawback",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "clawback" => Self::Clawback,
            _ => Self::Commission,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffiliateCode(String);

impl AffiliateCode {
    pub fn parse(raw: &str) -> AppResult<Self> {
        let code = raw.trim().to_ascii_lowercase();
        if code.len() < 3 || code.len() > 32 {
            return Err(AppError::Validation("error.affiliate_code_invalid"));
        }
        if code.starts_with('-') || code.ends_with('-') || code.contains("--") {
            return Err(AppError::Validation("error.affiliate_code_invalid"));
        }
        if !code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(AppError::Validation("error.affiliate_code_invalid"));
        }
        if RESERVED_CODES.contains(&code.as_str()) {
            return Err(AppError::Validation("error.affiliate_code_reserved"));
        }
        Ok(Self(code))
    }

    /// Generates a deterministic, URL-safe default referral code from an email prefix + random suffix.
    pub fn generate_default(email: &str, random_bytes: [u8; 4]) -> Self {
        let local = email.split('@').next().unwrap_or("parceiro");
        let mut prefix = String::new();
        let mut prev_dash = false;
        for ch in local.chars() {
            let lower = ch.to_ascii_lowercase();
            if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
                prefix.push(lower);
                prev_dash = false;
            } else if !prefix.is_empty() && !prev_dash {
                prefix.push('-');
                prev_dash = true;
            }
            if prefix.len() >= 16 {
                break;
            }
        }
        let prefix = prefix.trim_matches('-');
        let stem = if prefix.len() >= 2 { prefix } else { "ref" };
        let suffix = hex::encode(random_bytes);
        let candidate = format!("{stem}-{}", &suffix[..6]);
        Self::parse(&candidate).unwrap_or_else(|_| Self(format!("ref-{suffix}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct AffiliateProgramSettings {
    pub self_signup_enabled: bool,
    pub default_commission_type: CommissionType,
    pub default_commission_value: i64,
    pub default_hold_days: i64,
    pub cookie_ttl_days: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Affiliate {
    pub id: String,
    pub account_id: String,
    pub code: String,
    pub status: AffiliateStatus,
    pub commission_type: CommissionType,
    pub commission_value: i64,
    pub hold_days: i64,
    pub pix_key: String,
    pub notes: String,
    pub click_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_affiliate_codes() {
        assert_eq!(
            AffiliateCode::parse("  Joao-Silva99 ").unwrap().as_str(),
            "joao-silva99"
        );
        assert!(AffiliateCode::parse("ab").is_err());
        assert!(AffiliateCode::parse("-abc").is_err());
        assert!(AffiliateCode::parse("abc-").is_err());
        assert!(AffiliateCode::parse("ab--cd").is_err());
        assert!(AffiliateCode::parse("admin").is_err());
        assert!(AffiliateCode::parse("dashboard").is_err());
        assert!(AffiliateCode::parse("joao_silva").is_err());
    }
}
