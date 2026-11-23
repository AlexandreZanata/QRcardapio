use crate::error::{AppError, AppResult};

const MAX_SLUG_LEN: usize = 64;
const MAX_GENERATED_BASE_LEN: usize = 32;
const SHORT_CODE_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

const RESERVED_SLUGS: &[&str] = &[
    "admin",
    "api",
    "app",
    "apple-touch-icon",
    "assets",
    "auth",
    "c",
    "cardapio",
    "cardapio-digital",
    "dashboard",
    "digital-menu",
    "en",
    "es",
    "favicon",
    "ferramentas",
    "guias",
    "guides",
    "health",
    "healthz",
    "help",
    "herramientas",
    "i18n",
    "llms",
    "login",
    "logout",
    "manifest",
    "media",
    "menu",
    "menu-digital",
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
    "solucoes",
    "soluciones",
    "solutions",
    "static",
    "support",
    "terms",
    "tools",
    "www",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slug(String);

impl Slug {
    /// Normalizes a custom address while retaining the established validation rules.
    pub fn from_input(raw: &str) -> AppResult<Self> {
        Self::validate_existing(&normalize_ascii_slug(raw, MAX_SLUG_LEN))
    }

    /// Produces a compact address from any valid display name.
    pub fn from_name(name: &str) -> Self {
        let base = normalize_ascii_slug(name, MAX_GENERATED_BASE_LEN);
        if base.is_empty() {
            return Self("loja".into());
        }
        if base.len() < 3 || RESERVED_SLUGS.contains(&base.as_str()) {
            return Self(format!("{base}-loja"));
        }
        Self(base)
    }

    /// Extends a generated address with four unambiguous base-32 characters.
    pub fn with_short_code(&self, entropy: [u8; 4]) -> Self {
        let prefix = self.0[..self.0.len().min(MAX_SLUG_LEN - 5)].trim_end_matches('-');
        let mut suffix = String::with_capacity(4);
        for byte in entropy {
            suffix.push(char::from(SHORT_CODE_ALPHABET[usize::from(byte & 31)]));
        }
        Self(format!("{prefix}-{suffix}"))
    }

    pub fn validate_existing(raw: &str) -> AppResult<Self> {
        if !(3..=MAX_SLUG_LEN).contains(&raw.len()) {
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
        if RESERVED_SLUGS.contains(&raw) {
            return Err(AppError::Validation("error.slug_reserved"));
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

fn normalize_ascii_slug(raw: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(raw.len().min(max_len));
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
            '\u{0300}'..='\u{036f}' => continue,
            _ => None,
        };

        if let Some(c) = mapped {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() == max_len {
            break;
        }
    }

    out.truncate(out.trim_end_matches('-').len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_custom_addresses_and_builds_compact_automatic_addresses() {
        assert_eq!(
            Slug::from_input("Açaí do João & Cia!").unwrap().as_str(),
            "acai-do-joao-cia"
        );
        assert!(Slug::from_input("admin").is_err());
        assert!(Slug::from_input("ab").is_err());
        assert_eq!(Slug::from_name("Açaí").as_str(), "acai");
        assert_eq!(Slug::from_name("Ac\u{0327}ai\u{0301}").as_str(), "acai");
        assert_eq!(Slug::from_name("99").as_str(), "99-loja");
        assert_eq!(Slug::from_name("Menu").as_str(), "menu-loja");
        assert_eq!(Slug::from_name("🍕").as_str(), "loja");
        assert_eq!(
            Slug::from_name("Café")
                .with_short_code([9, 7, 20, 2])
                .as_str(),
            "cafe-97m2"
        );

        let names_with_long_prefixes = ["a".repeat(100), format!("{}{}", "a".repeat(31), " Café")];
        for name in names_with_long_prefixes {
            let base = Slug::from_name(&name);
            assert!(base.as_str().len() <= 32);
            let alternative = base.with_short_code([0; 4]);
            assert!(alternative.as_str().len() <= 37);
            Slug::validate_existing(alternative.as_str()).unwrap();
        }
        Slug::validate_existing(&"a".repeat(64)).unwrap();
    }
}
