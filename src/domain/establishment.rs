use crate::error::{AppError, AppResult};
use crate::i18n::{Locale, pick_localized_text};

const RESERVED_SLUGS: &[&str] = &[
    "admin",
    "api",
    "app",
    "assets",
    "auth",
    "c",
    "cardapio",
    "dashboard",
    "favicon",
    "health",
    "healthz",
    "help",
    "i18n",
    "login",
    "logout",
    "media",
    "menu",
    "onboarding",
    "pricing",
    "privacy",
    "published",
    "qr",
    "readyz",
    "register",
    "reset-password",
    "robots",
    "settings",
    "sitemap",
    "static",
    "support",
    "terms",
    "www",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slug(String);

impl Slug {
    /// Normalizes a user-supplied slug or establishment name into a URL-safe ASCII slug.
    pub fn from_input(raw: &str) -> AppResult<Self> {
        let normalized = normalize_ascii_slug(raw);
        Self::validate_normalized(&normalized)?;
        Ok(Self(normalized))
    }

    /// Generates a valid preview slug from an arbitrary establishment name.
    pub fn suggest_from_name(name: &str) -> String {
        normalize_ascii_slug(name)
    }

    pub fn validate_existing(raw: &str) -> AppResult<Self> {
        if raw.len() < 3 || raw.len() > 64 {
            return Err(AppError::Validation("error.slug_invalid"));
        }
        let valid_chars = raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !raw.starts_with('-')
            && !raw.ends_with('-')
            && !raw.contains("--");
        if !valid_chars {
            return Err(AppError::Validation("error.slug_invalid"));
        }
        if is_reserved_slug(raw) {
            return Err(AppError::Validation("error.slug_reserved"));
        }
        Ok(Self(raw.to_string()))
    }

    fn validate_normalized(s: &str) -> AppResult<()> {
        if s.len() < 3 || s.len() > 64 {
            return Err(AppError::Validation("error.slug_invalid"));
        }
        if is_reserved_slug(s) {
            return Err(AppError::Validation("error.slug_reserved"));
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn is_reserved_slug(slug: &str) -> bool {
    RESERVED_SLUGS.contains(&slug)
}

fn normalize_ascii_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_dash = true;

    for ch in raw.trim().chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => Some('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => Some('u'),
            'ç' | 'Ç' => Some('c'),
            'ñ' | 'Ñ' => Some('n'),
            _ => None,
        };

        if let Some(c) = mapped {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 64 {
        out.truncate(64);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccentColor(String);

impl AccentColor {
    pub const DEFAULT_HEX: &'static str = "#D9532F";

    pub fn parse(raw: &str) -> AppResult<Self> {
        let trimmed = raw.trim();
        let bytes = trimmed.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
        {
            return Err(AppError::Validation("error.invalid_color"));
        }
        Ok(Self(trimmed.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        let r = u8::from_str_radix(&self.0[1..3], 16).unwrap_or(217);
        let g = u8::from_str_radix(&self.0[3..5], 16).unwrap_or(83);
        let b = u8::from_str_radix(&self.0[5..7], 16).unwrap_or(47);
        (r, g, b)
    }

    /// Computes accessible foreground text color (`#1C1917` or `#FFFFFF`) using WCAG 2.1
    /// relative luminance so accent-colored badges and pills always satisfy contrast readability.
    pub fn contrast_ink(&self) -> &'static str {
        let (r, g, b) = self.rgb();
        let lum =
            0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b);
        if lum > 0.34 { "#1C1917" } else { "#FFFFFF" }
    }

    pub fn hover_hex(&self) -> String {
        let (r, g, b) = self.rgb();
        let darken = |c: u8| -> u8 { ((u16::from(c) * 85) / 100) as u8 };
        format!("#{:02X}{:02X}{:02X}", darken(r), darken(g), darken(b))
    }

    pub fn soft_tint_rgba(&self) -> String {
        let (r, g, b) = self.rgb();
        format!("rgba({r}, {g}, {b}, 0.12)")
    }
}

fn srgb_to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[derive(Debug, Clone)]
pub struct Establishment {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub accent_color: String,
    pub logo_asset_id: Option<String>,
    pub google_maps_url: Option<String>,
    pub google_review_url: Option<String>,
    pub is_published: bool,
    pub published_version: i64,
    pub has_unpublished_changes: bool,
    pub last_published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Establishment {
    pub fn effective_description(&self, locale: Locale) -> &str {
        pick_localized_text(
            locale,
            &self.description,
            &self.description_en,
            &self.description_es,
        )
    }
}

pub fn validate_non_empty_name(raw: &str, max_len: usize) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max_len {
        return Err(AppError::Validation("error.name_required"));
    }
    Ok(trimmed.to_string())
}

pub fn sanitize_optional_text(raw: &str, max_len: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= max_len {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max_len).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portuguese_slug_normalization() {
        let s = Slug::from_input("Açaí do João & Cia!").unwrap();
        assert_eq!(s.as_str(), "acai-do-joao-cia");
        assert!(Slug::from_input("admin").is_err());
        assert!(Slug::from_input("ab").is_err());
    }

    #[test]
    fn accent_color_contrast_selection() {
        let dark_coral = AccentColor::parse("#D9532F").unwrap();
        assert_eq!(dark_coral.contrast_ink(), "#FFFFFF");

        let bright_yellow = AccentColor::parse("#FACC15").unwrap();
        assert_eq!(bright_yellow.contrast_ink(), "#1C1917");

        assert!(AccentColor::parse("red").is_err());
    }
}
