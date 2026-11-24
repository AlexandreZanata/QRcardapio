pub mod faq;
pub mod guides;
pub mod routes;
pub mod schema;
pub mod segments;
pub mod sitemap;
pub mod tools;

use serde::Serialize;

use crate::i18n::Locale;

pub const SEO_LASTMOD: &str = "2026-11-23";

#[derive(Debug, Clone)]
pub struct HreflangLink {
    pub hreflang: &'static str,
    pub href: String,
}

#[derive(Debug, Clone)]
pub struct SeoMeta {
    pub robots: &'static str,
    pub canonical: String,
    pub alternates: Vec<HreflangLink>,
    pub title_override: Option<String>,
    pub description_key: &'static str,
    pub description_override: Option<String>,
    pub emit_social: bool,
    pub og_type: &'static str,
    pub og_locale: &'static str,
    pub og_locale_alternates: Vec<&'static str>,
    pub og_image: String,
    pub og_image_alt: String,
    pub json_ld: String,
}

impl SeoMeta {
    pub fn noindex(description_key: &'static str) -> Self {
        Self {
            robots: "noindex, nofollow",
            canonical: String::new(),
            alternates: Vec::new(),
            title_override: None,
            description_key,
            description_override: None,
            emit_social: false,
            og_type: "website",
            og_locale: "pt_BR",
            og_locale_alternates: Vec::new(),
            og_image: String::new(),
            og_image_alt: String::new(),
            json_ld: String::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn indexable(
        base_url: &str,
        locale: Locale,
        canonical_path: &str,
        alternates: Vec<HreflangLink>,
        title_override: Option<String>,
        description_key: &'static str,
        description_override: Option<String>,
        og_type: &'static str,
        og_image_alt: String,
        json_ld: String,
    ) -> Self {
        let base = normalized_base_url(base_url);
        let (og_locale, og_locale_alternates) = match locale {
            Locale::PtBr => ("pt_BR", vec!["en_US", "es_ES"]),
            Locale::En => ("en_US", vec!["pt_BR", "es_ES"]),
            Locale::Es => ("es_ES", vec!["pt_BR", "en_US"]),
        };
        Self {
            robots: "index, follow, max-image-preview:large, max-snippet:-1, max-video-preview:-1",
            canonical: absolute_url(base, canonical_path),
            alternates,
            title_override,
            description_key,
            description_override,
            emit_social: true,
            og_type,
            og_locale,
            og_locale_alternates,
            og_image: og_image_url(base, locale),
            og_image_alt,
            json_ld,
        }
    }
}

pub fn normalized_base_url(raw: &str) -> &str {
    raw.trim_end_matches('/')
}

pub fn absolute_url(base: &str, path: &str) -> String {
    let base = normalized_base_url(base);
    if path == "/" || path.is_empty() {
        format!("{base}/")
    } else if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

pub fn og_image_url(base: &str, locale: Locale) -> String {
    format!(
        "{}/static/images/og-{}.png",
        normalized_base_url(base),
        locale.code()
    )
}

/// Serializes JSON-LD safely for embedding inside `<script type="application/ld+json">`.
pub fn safe_json_ld<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

/// Formats integer centavos as decimal `"29.90"` using integer arithmetic only.
pub fn format_decimal_price(centavos: i64) -> String {
    let neg = if centavos < 0 { "-" } else { "" };
    let abs = centavos.unsigned_abs();
    format!("{neg}{}.{:02}", abs / 100, abs % 100)
}
