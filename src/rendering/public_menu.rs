use askama::Template;
use sha2::{Digest, Sha256};

use crate::domain::establishment::AccentColor;
use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale, format_price};

pub struct PublicProductView {
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub display_name: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub display_description: String,
    pub price_centavos: i64,
    pub price_display: String,
    pub image_url: Option<String>,
    pub is_available: bool,
}

pub struct PublicCategoryView {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub display_name: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub display_description: String,
    pub products: Vec<PublicProductView>,
}

#[derive(Template)]
#[template(path = "menu/public_menu.html")]
pub struct PublicMenuTemplate {
    pub locale_code: &'static str,
    pub establishment_name: String,
    pub establishment_description: String,
    pub establishment_description_en: String,
    pub establishment_description_es: String,
    pub establishment_display_description: String,
    pub logo_url: Option<String>,
    pub cover_url: Option<String>,
    pub theme_css_href: String,
    pub is_preview: bool,
    pub skip_label: String,
    pub switcher_label: String,
    pub preview_banner_text: String,
    pub search_placeholder: String,
    pub categories_label: String,
    pub all_categories_text: String,
    pub unavailable_text: String,
    pub empty_menu_text: String,
    pub no_results_text: String,
    pub powered_by_text: String,
    pub categories: Vec<PublicCategoryView>,
    pub menu_i18n_json: String,
}

/// Generates a minimal external CSS stylesheet defining the establishment's brand accent variables.
/// Returns `(filename, css_bytes)` where `filename` is `theme-{hash8}.css`.
pub fn generate_brand_theme_css(accent_hex: &str) -> (String, Vec<u8>) {
    let color = AccentColor::parse(accent_hex)
        .unwrap_or_else(|_| AccentColor::parse(AccentColor::DEFAULT_HEX).expect("valid default"));
    let css = format!(
        ":root{{--brand-accent:{};--brand-accent-hover:{};--brand-accent-ink:{};--brand-soft:{};}}\n",
        color.as_str(),
        color.hover_hex(),
        color.contrast_ink(),
        color.soft_tint_rgba(),
    );
    let mut hasher = Sha256::new();
    hasher.update(css.as_bytes());
    let hash = hex::encode(hasher.finalize());
    let filename = format!("theme-{}.css", &hash[..8]);
    (filename, css.into_bytes())
}

pub fn render_public_menu_html(
    snapshot: &MenuSnapshot,
    locale: Locale,
    is_preview: bool,
    media_base_url: &str,
    theme_css_href: &str,
) -> AppResult<String> {
    let cat = Catalog::global();
    let est = &snapshot.establishment;

    let logo_url = snapshot
        .logo
        .as_ref()
        .map(|l| format!("{media_base_url}/{}", l.filename_thumb));
    let cover_url = snapshot
        .cover_image()
        .map(|cover| format!("{media_base_url}/{}", cover.filename_large));

    let categories = snapshot
        .categories
        .iter()
        .filter(|sc| !sc.products.is_empty() || is_preview)
        .map(|sc| {
            let products = sc
                .products
                .iter()
                .map(|sp| {
                    let p = &sp.product;
                    let image_url = sp
                        .image
                        .as_ref()
                        .map(|m| format!("{media_base_url}/{}", m.filename_thumb));
                    PublicProductView {
                        name: p.name.clone(),
                        name_en: p.name_en.clone(),
                        name_es: p.name_es.clone(),
                        display_name: p.effective_name(locale).to_string(),
                        description: p.description.clone(),
                        description_en: p.description_en.clone(),
                        description_es: p.description_es.clone(),
                        display_description: p.effective_description(locale).to_string(),
                        price_centavos: p.price_centavos,
                        price_display: format_price(p.price_centavos, locale),
                        image_url,
                        is_available: p.is_available,
                    }
                })
                .collect();

            PublicCategoryView {
                id: sc.category.id.clone(),
                name: sc.category.name.clone(),
                name_en: sc.category.name_en.clone(),
                name_es: sc.category.name_es.clone(),
                display_name: sc.category.effective_name(locale).to_string(),
                description: sc.category.description.clone(),
                description_en: sc.category.description_en.clone(),
                description_es: sc.category.description_es.clone(),
                display_description: sc.category.effective_description(locale).to_string(),
                products,
            }
        })
        .collect();

    let tpl = PublicMenuTemplate {
        locale_code: locale.code(),
        establishment_name: est.name.clone(),
        establishment_description: est.description.clone(),
        establishment_description_en: est.description_en.clone(),
        establishment_description_es: est.description_es.clone(),
        establishment_display_description: est.effective_description(locale).to_string(),
        logo_url,
        cover_url,
        theme_css_href: theme_css_href.to_string(),
        is_preview,
        skip_label: cat.t(locale, "common.skip_to_content").to_string(),
        switcher_label: cat.t(locale, "lang.switcher_label").to_string(),
        preview_banner_text: cat.t(locale, "public_menu.preview_banner").to_string(),
        search_placeholder: cat.t(locale, "public_menu.search_placeholder").to_string(),
        categories_label: cat.t(locale, "public_menu.categories_label").to_string(),
        all_categories_text: cat.t(locale, "public_menu.all_categories").to_string(),
        unavailable_text: cat.t(locale, "public_menu.unavailable_badge").to_string(),
        empty_menu_text: cat.t(locale, "public_menu.empty_menu").to_string(),
        no_results_text: cat.t(locale, "public_menu.no_results").to_string(),
        powered_by_text: cat.t(locale, "public_menu.powered_by").to_string(),
        categories,
        menu_i18n_json: cat.public_menu_embedded_json().to_string(),
    };

    tpl.render()
        .map_err(|e| AppError::Internal(format!("public menu render failed: {e}")))
}
