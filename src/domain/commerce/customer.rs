use serde::{Deserialize, Serialize};

use crate::domain::account::Email;
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize, Serialize)]
pub struct CustomerProfile {
    pub name: String,
    pub phone_e164: String,
    #[serde(default)]
    pub email: String,
}

impl CustomerProfile {
    pub fn normalized(self) -> AppResult<Self> {
        let email = self.email.trim();
        Ok(Self {
            name: bounded_text(&self.name, 80, true, "commerce.customer.invalid")?,
            phone_e164: normalize_phone(&self.phone_e164)?,
            email: if email.is_empty() {
                String::new()
            } else {
                bounded_text(
                    Email::parse(email)?.as_str(),
                    254,
                    true,
                    "error.invalid_email",
                )?
            },
        })
    }
}

pub fn normalize_phone(raw: &str) -> AppResult<String> {
    const INVALID: AppError = AppError::Validation("commerce.customer.invalid_phone");
    let raw = raw.trim();
    if raw.len() > 32 {
        return Err(INVALID);
    }
    let (explicit_country, rest) = match raw.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let mut digits = String::with_capacity(15);
    for character in rest.chars() {
        match character {
            '0'..='9' => digits.push(character),
            ' ' | '(' | ')' | '-' | '.' => (),
            _ => return Err(INVALID),
        }
    }
    if !explicit_country {
        if matches!(digits.len(), 10 | 11) && !digits.starts_with('0') {
            digits.insert_str(0, "55");
        } else if !matches!(digits.len(), 12 | 13) || !digits.starts_with("55") {
            return Err(INVALID);
        }
    }
    if !(8..=15).contains(&digits.len()) || digits.starts_with('0') {
        return Err(INVALID);
    }
    Ok(format!("+{digits}"))
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PostalAddress {
    pub street: String,
    pub street_number: String,
    #[serde(default)]
    pub complement: String,
    pub neighborhood: String,
    pub city: String,
    pub uf: String,
    pub cep: String,
    #[serde(default)]
    pub reference: String,
}

pub const FEDERAL_UNITS: [&str; 27] = [
    "AC", "AL", "AP", "AM", "BA", "CE", "DF", "ES", "GO", "MA", "MT", "MS", "MG", "PA", "PB", "PR",
    "PE", "PI", "RJ", "RN", "RS", "RO", "RR", "SC", "SP", "SE", "TO",
];

impl PostalAddress {
    pub fn normalized(self) -> AppResult<Self> {
        const INVALID: &str = "commerce.customer.invalid_address";
        let text = |raw: &str, max, required| bounded_text(raw, max, required, INVALID);
        let uf = self.uf.trim().to_ascii_uppercase();
        let raw_cep = self.cep.trim();
        if !FEDERAL_UNITS.contains(&uf.as_str())
            || !matches!(raw_cep.len(), 8 | 9)
            || (raw_cep.len() == 9 && raw_cep.as_bytes()[5] != b'-')
        {
            return Err(AppError::Validation(INVALID));
        }
        let cep = raw_cep.replace('-', "");
        if cep.len() != 8 || !cep.bytes().all(|part| part.is_ascii_digit()) {
            return Err(AppError::Validation(INVALID));
        }
        Ok(Self {
            street: text(&self.street, 120, true)?,
            street_number: text(&self.street_number, 20, true)?,
            complement: text(&self.complement, 80, false)?,
            neighborhood: text(&self.neighborhood, 80, true)?,
            city: text(&self.city, 80, true)?,
            uf,
            cep,
            reference: text(&self.reference, 140, false)?,
        })
    }
}

fn bounded_text(raw: &str, max: usize, required: bool, error: &'static str) -> AppResult<String> {
    let text = raw.trim();
    if (required && text.is_empty())
        || text.chars().count() > max
        || text.chars().any(char::is_control)
    {
        return Err(AppError::Validation(error));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_national_and_international_phones() {
        assert_eq!(
            normalize_phone("(65) 99999-0020").unwrap(),
            "+5565999990020"
        );
        assert_eq!(
            normalize_phone("55 65 99999-0020").unwrap(),
            "+5565999990020"
        );
        assert_eq!(
            normalize_phone("+1 (201) 555-0123").unwrap(),
            "+12015550123"
        );
        assert!(normalize_phone("00 1 2015550123").is_err());
        assert!(normalize_phone("+55 65 99999 0020 ramal 43").is_err());
    }
}
