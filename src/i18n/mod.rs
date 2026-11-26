use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::LazyLock;

const PT_BR_JSON: &str = include_str!("../../i18n/pt-BR.json");
const EN_JSON: &str = include_str!("../../i18n/en.json");
const ES_JSON: &str = include_str!("../../i18n/es.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    #[default]
    PtBr,
    En,
    Es,
}

impl Locale {
    pub const ALL: [Locale; 3] = [Locale::PtBr, Locale::En, Locale::Es];

    pub const fn code(self) -> &'static str {
        match self {
            Self::PtBr => "pt-BR",
            Self::En => "en",
            Self::Es => "es",
        }
    }

    pub const fn html_lang(self) -> &'static str {
        self.code()
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::PtBr => "PT",
            Self::En => "EN",
            Self::Es => "ES",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let norm = raw.trim().to_ascii_lowercase().replace('_', "-");
        if norm == "pt" || norm == "pt-br" || norm.starts_with("pt-") {
            Some(Self::PtBr)
        } else if norm == "en" || norm.starts_with("en-") {
            Some(Self::En)
        } else if norm == "es" || norm.starts_with("es-") {
            Some(Self::Es)
        } else {
            None
        }
    }

    /// Parses an HTTP `Accept-Language` header respecting quality weights (`q=`).
    pub fn from_accept_language(header: &str) -> Option<Self> {
        let mut best: Option<(u16, usize, Self)> = None;
        for (idx, part) in header.split(',').enumerate() {
            let mut segments = part.trim().split(';');
            let tag = segments.next().unwrap_or("").trim();
            if tag.is_empty() || tag == "*" {
                continue;
            }
            let Some(locale) = Self::parse(tag) else {
                continue;
            };
            let mut q_thousand: u16 = 1000;
            for param in segments {
                let p = param.trim();
                if let Some(q_val) = p.strip_prefix("q=")
                    && let Ok(parsed) = q_val.trim().parse::<f32>()
                {
                    let clamped = parsed.clamp(0.0, 1.0);
                    q_thousand = (clamped * 1000.0).round() as u16;
                }
            }
            if q_thousand == 0 {
                continue;
            }
            let replace = match best {
                None => true,
                Some((best_q, best_idx, _)) => {
                    q_thousand > best_q || (q_thousand == best_q && idx < best_idx)
                }
            };
            if replace {
                best = Some((q_thousand, idx, locale));
            }
        }
        best.map(|(_, _, loc)| loc)
    }
}

#[derive(Debug)]
pub struct Catalog {
    pt_br: BTreeMap<String, String>,
    en: BTreeMap<String, String>,
    es: BTreeMap<String, String>,
    version_hash: String,
    public_menu_embedded_json: String,
    storefront_embedded_json: String,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(Catalog::build);

impl Catalog {
    pub fn global() -> &'static Catalog {
        &CATALOG
    }

    fn build() -> Self {
        let pt_br: BTreeMap<String, String> =
            serde_json::from_str(PT_BR_JSON).expect("valid i18n/pt-BR.json");
        let en: BTreeMap<String, String> =
            serde_json::from_str(EN_JSON).expect("valid i18n/en.json");
        let es: BTreeMap<String, String> =
            serde_json::from_str(ES_JSON).expect("valid i18n/es.json");

        let mut hasher = Sha256::new();
        hasher.update(PT_BR_JSON.as_bytes());
        hasher.update(EN_JSON.as_bytes());
        hasher.update(ES_JSON.as_bytes());
        let full_hash = hex::encode(hasher.finalize());
        let version_hash = full_hash[..12].to_string();

        // Compact subset embedded directly inside published static menus so public QR scans
        // can switch between PT / EN / ES with zero reload and zero extra network requests.
        let mut menu_bundle: BTreeMap<&str, BTreeMap<&str, &str>> = BTreeMap::new();
        for locale in Locale::ALL {
            let src = match locale {
                Locale::PtBr => &pt_br,
                Locale::En => &en,
                Locale::Es => &es,
            };
            let mut subset = BTreeMap::new();
            for (k, v) in src {
                if k.starts_with("public_menu.")
                    || k.starts_with("lang.")
                    || k == "common.skip_to_content"
                    || k == "app.name"
                    || k == "commerce.catalog.variations"
                {
                    subset.insert(k.as_str(), v.as_str());
                }
            }
            menu_bundle.insert(locale.code(), subset);
        }
        // Escape `</` as `<\/` so embedding inside `<script type="application/json">` is always safe.
        let public_menu_embedded_json = serde_json::to_string(&menu_bundle)
            .expect("serialize menu i18n bundle")
            .replace("</", "<\\/");

        let mut storefront_bundle: BTreeMap<&str, BTreeMap<&str, &str>> = BTreeMap::new();
        for locale in Locale::ALL {
            let src = match locale {
                Locale::PtBr => &pt_br,
                Locale::En => &en,
                Locale::Es => &es,
            };
            let mut subset = BTreeMap::new();
            for (k, v) in src {
                if k.starts_with("storefront.")
                    || k.starts_with("delivery.")
                    || k.starts_with("public_menu.")
                    || k.starts_with("lang.")
                    || k.starts_with("error.")
                    || k.starts_with("commerce.")
                    || matches!(
                        k.as_str(),
                        "app.name"
                            | "billing.method_heading"
                            | "common.back"
                            | "common.close"
                            | "common.skip_to_content"
                    )
                {
                    subset.insert(k.as_str(), v.as_str());
                }
            }
            storefront_bundle.insert(locale.code(), subset);
        }
        let storefront_embedded_json = serde_json::to_string(&storefront_bundle)
            .expect("serialize storefront i18n bundle")
            .replace("</", "<\\/");

        Self {
            pt_br,
            en,
            es,
            version_hash,
            public_menu_embedded_json,
            storefront_embedded_json,
        }
    }

    pub fn map_for(&self, locale: Locale) -> &BTreeMap<String, String> {
        match locale {
            Locale::PtBr => &self.pt_br,
            Locale::En => &self.en,
            Locale::Es => &self.es,
        }
    }

    pub fn t<'a>(&'a self, locale: Locale, key: &'a str) -> &'a str {
        self.map_for(locale)
            .get(key)
            .or_else(|| self.pt_br.get(key))
            .map(String::as_str)
            .unwrap_or(key)
    }

    pub fn raw_json(&self, locale: Locale) -> &'static str {
        match locale {
            Locale::PtBr => PT_BR_JSON,
            Locale::En => EN_JSON,
            Locale::Es => ES_JSON,
        }
    }

    pub fn version_hash(&self) -> &str {
        &self.version_hash
    }

    pub fn public_menu_embedded_json(&self) -> &str {
        &self.public_menu_embedded_json
    }

    pub fn storefront_embedded_json(&self) -> &str {
        &self.storefront_embedded_json
    }
}

/// Deterministic integer currency formatting (Brazilian Real) matching frontend JS formatting.
/// Examples: `2990` -> `"R$ 29,90"` (pt-BR/es) or `"R$ 29.90"` (en).
pub fn format_price(centavos: i64, locale: Locale) -> String {
    let negative = centavos < 0;
    let abs = centavos.unsigned_abs();
    let integer_part = abs / 100;
    let frac_part = abs % 100;

    let (thousands_sep, decimal_sep) = match locale {
        Locale::PtBr | Locale::Es => ('.', ','),
        Locale::En => (',', '.'),
    };

    let int_str = integer_part.to_string();
    let mut grouped = String::with_capacity(int_str.len() + int_str.len() / 3);
    for (idx, ch) in int_str.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push(thousands_sep);
        }
        grouped.push(ch);
    }
    let grouped_fwd: String = grouped.chars().rev().collect();

    if negative {
        format!("-R$ {grouped_fwd}{decimal_sep}{frac_part:02}")
    } else {
        format!("R$ {grouped_fwd}{decimal_sep}{frac_part:02}")
    }
}

/// Picks the localized optional field if present and non-empty, falling back to the primary text.
pub fn pick_localized_text<'a>(
    locale: Locale,
    primary: &'a str,
    en: &'a str,
    es: &'a str,
) -> &'a str {
    match locale {
        Locale::PtBr => primary,
        Locale::En => {
            if en.trim().is_empty() {
                primary
            } else {
                en
            }
        }
        Locale::Es => {
            if es.trim().is_empty() {
                primary
            } else {
                es
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn catalogs_have_identical_keys() {
        let cat = Catalog::global();
        let k_pt: BTreeSet<&String> = cat.map_for(Locale::PtBr).keys().collect();
        let k_en: BTreeSet<&String> = cat.map_for(Locale::En).keys().collect();
        let k_es: BTreeSet<&String> = cat.map_for(Locale::Es).keys().collect();
        assert_eq!(k_pt, k_en, "pt-BR and en keys diverged");
        assert_eq!(k_pt, k_es, "pt-BR and es keys diverged");
        assert!(!k_pt.is_empty());
    }

    #[test]
    fn accept_language_quality_resolution() {
        assert_eq!(
            Locale::from_accept_language("fr-FR,en-US;q=0.9,pt-BR;q=0.8"),
            Some(Locale::En)
        );
        assert_eq!(
            Locale::from_accept_language("es-419,es;q=0.9,en;q=0.7"),
            Some(Locale::Es)
        );
        assert_eq!(
            Locale::from_accept_language("pt-BR,pt;q=0.9,en;q=0.8"),
            Some(Locale::PtBr)
        );
        assert_eq!(Locale::from_accept_language("ja-JP"), None);
    }

    #[test]
    fn format_price_integer_precision() {
        assert_eq!(format_price(0, Locale::PtBr), "R$ 0,00");
        assert_eq!(format_price(2990, Locale::PtBr), "R$ 29,90");
        assert_eq!(format_price(2990, Locale::En), "R$ 29.90");
        assert_eq!(format_price(123456, Locale::PtBr), "R$ 1.234,56");
        assert_eq!(format_price(123456, Locale::En), "R$ 1,234.56");
    }
}
