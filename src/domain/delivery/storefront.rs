use crate::domain::commerce::order_payment::OfflineMethod;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorefrontStatus {
    Draft,
    Published,
    Unpublished,
}

impl StorefrontStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Unpublished => "unpublished",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(Self::Draft),
            "published" => Some(Self::Published),
            "unpublished" => Some(Self::Unpublished),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorefrontSettings {
    pub establishment_id: String,
    pub status: StorefrontStatus,
    pub published_version: i64,
    pub has_unpublished_changes: bool,
    pub last_published_at: Option<String>,
    pub accept_cash: bool,
    pub accept_card_on_delivery: bool,
    pub accept_pix_on_delivery: bool,
    pub pix_key_display: String,
    pub contact_whatsapp: String,
    pub pickup_instructions: String,
    pub default_prep_minutes: i64,
    pub updated_at: String,
}

impl StorefrontSettings {
    pub fn default_for(establishment_id: &str) -> Self {
        Self {
            establishment_id: establishment_id.to_owned(),
            status: StorefrontStatus::Draft,
            published_version: 0,
            has_unpublished_changes: true,
            last_published_at: None,
            accept_cash: true,
            accept_card_on_delivery: true,
            accept_pix_on_delivery: false,
            pix_key_display: String::new(),
            contact_whatsapp: String::new(),
            pickup_instructions: String::new(),
            default_prep_minutes: 25,
            updated_at: String::new(),
        }
    }

    pub fn accepts_method(&self, method: OfflineMethod) -> bool {
        match method {
            OfflineMethod::Cash => self.accept_cash,
            OfflineMethod::CardOnDelivery => self.accept_card_on_delivery,
            OfflineMethod::PixOnDelivery => {
                self.accept_pix_on_delivery && !self.pix_key_display.trim().is_empty()
            }
        }
    }
}

pub struct StorefrontSettingsInput<'a> {
    pub accept_cash: bool,
    pub accept_card_on_delivery: bool,
    pub accept_pix_on_delivery: bool,
    pub pix_key_display: &'a str,
    pub contact_whatsapp: &'a str,
    pub pickup_instructions: &'a str,
    pub default_prep_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedStorefrontSettings {
    pub accept_cash: bool,
    pub accept_card_on_delivery: bool,
    pub accept_pix_on_delivery: bool,
    pub pix_key_display: String,
    pub contact_whatsapp: String,
    pub pickup_instructions: String,
    pub default_prep_minutes: i64,
}

impl ValidatedStorefrontSettings {
    pub fn validate(input: StorefrontSettingsInput<'_>) -> AppResult<Self> {
        if !input.accept_cash && !input.accept_card_on_delivery && !input.accept_pix_on_delivery {
            return Err(AppError::Validation("delivery.invalid_payment_settings"));
        }
        let pix_key_display = validate_pix_key(input.pix_key_display)?;
        if input.accept_pix_on_delivery && pix_key_display.is_empty() {
            return Err(AppError::Validation("delivery.pix_key_required"));
        }
        let contact_whatsapp = validate_whatsapp(input.contact_whatsapp)?;
        let pickup_instructions = validate_pickup_instructions(input.pickup_instructions)?;
        if !(5..=180).contains(&input.default_prep_minutes) {
            return Err(AppError::Validation("delivery.invalid_prep_minutes"));
        }
        Ok(Self {
            accept_cash: input.accept_cash,
            accept_card_on_delivery: input.accept_card_on_delivery,
            accept_pix_on_delivery: input.accept_pix_on_delivery,
            pix_key_display,
            contact_whatsapp,
            pickup_instructions,
            default_prep_minutes: input.default_prep_minutes,
        })
    }
}

pub fn validate_pix_key(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > 140 || trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation("delivery.invalid_pix_key"));
    }
    Ok(trimmed.to_owned())
}

pub fn validate_whatsapp(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > 32 {
        return Err(AppError::Validation("delivery.invalid_whatsapp"));
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    if !(10..=15).contains(&digits.len())
        || digits.starts_with('0')
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-' | '(' | ')'))
    {
        return Err(AppError::Validation("delivery.invalid_whatsapp"));
    }
    if digits.len() <= 11 && !digits.starts_with("55") {
        Ok(format!("55{digits}"))
    } else {
        Ok(digits)
    }
}

pub fn validate_pickup_instructions(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > 280 || trimmed.chars().any(char::is_control) {
        return Err(AppError::Validation("delivery.invalid_pickup_instructions"));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_storefront_payment_methods_and_whatsapp() {
        assert!(
            ValidatedStorefrontSettings::validate(StorefrontSettingsInput {
                accept_cash: false,
                accept_card_on_delivery: false,
                accept_pix_on_delivery: false,
                pix_key_display: "",
                contact_whatsapp: "",
                pickup_instructions: "",
                default_prep_minutes: 25,
            })
            .is_err()
        );
        assert!(
            ValidatedStorefrontSettings::validate(StorefrontSettingsInput {
                accept_cash: true,
                accept_card_on_delivery: false,
                accept_pix_on_delivery: true,
                pix_key_display: "",
                contact_whatsapp: "",
                pickup_instructions: "",
                default_prep_minutes: 25,
            })
            .is_err()
        );
        let ok = ValidatedStorefrontSettings::validate(StorefrontSettingsInput {
            accept_cash: true,
            accept_card_on_delivery: true,
            accept_pix_on_delivery: true,
            pix_key_display: "pix@bistro.com.br",
            contact_whatsapp: "(65) 99999-0011",
            pickup_instructions: "Retirar no balcao",
            default_prep_minutes: 30,
        })
        .unwrap();
        assert_eq!(ok.contact_whatsapp, "5565999990011");
    }
}
