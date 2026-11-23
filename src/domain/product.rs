use crate::error::{AppError, AppResult};
use crate::i18n::{Locale, pick_localized_text};

pub const MAX_PRICE_CENTAVOS: i64 = 99_999_999;

/// Parses a decimal price string into integer centavos without floating-point arithmetic.
/// Supports Brazilian (`29,90`, `1.250,00`, `R$ 19,90`) and dot (`29.90`, `15`) formats.
pub fn parse_price_centavos(raw: &str) -> AppResult<i64> {
    let cleaned = raw
        .trim()
        .trim_start_matches("R$")
        .trim_start_matches("r$")
        .trim();

    if cleaned.is_empty() || cleaned.starts_with('-') {
        return Err(AppError::Validation("error.invalid_price"));
    }

    if !cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    {
        return Err(AppError::Validation("error.invalid_price"));
    }

    let last_comma = cleaned.rfind(',');
    let last_dot = cleaned.rfind('.');

    let decimal_pos = match (last_comma, last_dot) {
        (Some(c), Some(d)) => {
            let sep = c.max(d);
            let frac_len = cleaned.len() - sep - 1;
            if frac_len > 2 {
                return Err(AppError::Validation("error.invalid_price"));
            }
            Some(sep)
        }
        (Some(c), None) => {
            let frac_len = cleaned.len() - c - 1;
            let count = cleaned.matches(',').count();
            if count == 1 && (1..=2).contains(&frac_len) {
                Some(c)
            } else {
                return Err(AppError::Validation("error.invalid_price"));
            }
        }
        (None, Some(d)) => {
            let frac_len = cleaned.len() - d - 1;
            let count = cleaned.matches('.').count();
            if count == 1 && (1..=2).contains(&frac_len) {
                Some(d)
            } else {
                return Err(AppError::Validation("error.invalid_price"));
            }
        }
        (None, None) => None,
    };

    let (int_raw, frac_raw) = match decimal_pos {
        Some(pos) => (&cleaned[..pos], &cleaned[pos + 1..]),
        None => (cleaned, ""),
    };

    let int_digits: String = int_raw.chars().filter(char::is_ascii_digit).collect();
    if int_digits.is_empty() || frac_raw.len() > 2 || !frac_raw.chars().all(|c| c.is_ascii_digit())
    {
        return Err(AppError::Validation("error.invalid_price"));
    }

    let whole: i64 = int_digits
        .parse()
        .map_err(|_| AppError::Validation("error.invalid_price"))?;

    let cents: i64 = match frac_raw.len() {
        0 => 0,
        1 => {
            let d: i64 = frac_raw
                .parse()
                .map_err(|_| AppError::Validation("error.invalid_price"))?;
            d * 10
        }
        2 => frac_raw
            .parse()
            .map_err(|_| AppError::Validation("error.invalid_price"))?,
        _ => return Err(AppError::Validation("error.invalid_price")),
    };

    let total = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(cents))
        .ok_or(AppError::Validation("error.invalid_price"))?;

    if total > MAX_PRICE_CENTAVOS {
        return Err(AppError::Validation("error.invalid_price"));
    }

    Ok(total)
}

/// Formats integer centavos for editable price form inputs (e.g. `2990` -> `"29,90"`).
pub fn format_price_input(centavos: i64) -> String {
    let abs = centavos.max(0);
    format!("{},{:02}", abs / 100, abs % 100)
}

#[derive(Debug, Clone)]
pub struct Product {
    pub id: String,
    pub establishment_id: String,
    pub category_id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub price_centavos: i64,
    pub image_asset_id: Option<String>,
    pub position: i64,
    pub is_available: bool,
    pub is_visible: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Product {
    pub fn effective_name(&self, locale: Locale) -> &str {
        pick_localized_text(locale, &self.name, &self.name_en, &self.name_es)
    }

    pub fn effective_description(&self, locale: Locale) -> &str {
        pick_localized_text(
            locale,
            &self.description,
            &self.description_en,
            &self.description_es,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_inputs_without_floats() {
        assert_eq!(parse_price_centavos("29,90").unwrap(), 2990);
        assert_eq!(parse_price_centavos("29.90").unwrap(), 2990);
        assert_eq!(parse_price_centavos("R$ 18,5").unwrap(), 1850);
        assert_eq!(parse_price_centavos("15").unwrap(), 1500);
        assert_eq!(parse_price_centavos("1.250,00").unwrap(), 125000);
        assert_eq!(parse_price_centavos("1,250.50").unwrap(), 125050);
        assert_eq!(parse_price_centavos("0,00").unwrap(), 0);
        assert!(parse_price_centavos("-5,00").is_err());
        assert!(parse_price_centavos("abc").is_err());
        assert!(parse_price_centavos("10,999").is_err());
    }
}
