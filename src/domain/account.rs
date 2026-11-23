use crate::error::{AppError, AppResult};
use crate::i18n::Locale;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountRole {
    Owner,
    Admin,
}

impl AccountRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        if raw == "admin" {
            Self::Admin
        } else {
            Self::Owner
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> AppResult<Self> {
        let trimmed = raw.trim().to_ascii_lowercase();
        if trimmed.len() < 5 || trimmed.len() > 254 || trimmed.contains(char::is_whitespace) {
            return Err(AppError::Validation("error.invalid_email"));
        }
        let Some((local, domain)) = trimmed.split_once('@') else {
            return Err(AppError::Validation("error.invalid_email"));
        };
        if local.is_empty()
            || domain.len() < 3
            || domain.contains('@')
            || !domain.contains('.')
            || domain.starts_with('.')
            || domain.ends_with('.')
            || domain.contains("..")
        {
            return Err(AppError::Validation("error.invalid_email"));
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn validate_password(password: &str) -> AppResult<()> {
    let char_count = password.chars().count();
    if !(8..=128).contains(&char_count) {
        return Err(AppError::Validation("error.password_length"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub role: AccountRole,
    pub preferred_locale: Locale,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization_and_validation() {
        let e = Email::parse("  Maria.Silva@Cafe.COM.br ").unwrap();
        assert_eq!(e.as_str(), "maria.silva@cafe.com.br");
        assert!(Email::parse("invalid").is_err());
        assert!(Email::parse("a@b").is_err());
        assert!(Email::parse("a@b..com").is_err());
        assert!(Email::parse("a b@c.com").is_err());
    }

    #[test]
    fn password_bounds() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"a".repeat(129)).is_err());
    }
}
