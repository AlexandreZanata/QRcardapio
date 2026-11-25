use askama::Template;
use sha2::{Digest, Sha256};

use crate::domain::establishment::{AccentColor, PUBLIC_DEMO_SLUG};
use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale, format_price};
use crate::seo::schema::public_menu_json_ld;
use crate::seo::{absolute_url, normalized_base_url, og_image_url};

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
    pub removals: Vec<ChoiceView>,
    pub extras: Vec<ChoiceView>,
}

pub struct ChoiceView {
    pub name: (&'static str, &'static str, &'static str),
    pub display_name: &'static str,
    pub price: Option<(i64, String)>,
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
    pub google_review_target: Option<String>,
    pub google_review_heading: String,
    pub google_review_body: String,
    pub google_review_action: String,
    pub theme_css_href: String,
    pub is_preview: bool,
    pub skip_label: String,
    pub switcher_label: String,
    pub preview_banner_text: String,
    pub search_placeholder: String,
    pub categories_label: String,
    pub all_categories_text: String,
    pub unavailable_text: String,
    pub choices_text: String,
    pub removable_text: String,
    pub extras_text: String,
    pub empty_menu_text: String,
    pub no_results_text: String,
    pub powered_by_text: String,
    pub categories: Vec<PublicCategoryView>,
    pub menu_i18n_json: String,
    pub robots_meta: &'static str,
    pub canonical_url: String,
    pub meta_description: String,
    pub og_image_url: String,
    pub json_ld: String,
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
    public_base_url: &str,
) -> AppResult<String> {
    let cat = Catalog::global();
    let est = &snapshot.establishment;
    let is_demo = est.slug == PUBLIC_DEMO_SLUG;

    let logo_url = snapshot
        .logo
        .as_ref()
        .map(|l| format!("{media_base_url}/{}", l.filename_thumb));
    let cover_url = snapshot
        .cover_image()
        .map(|cover| format!("{media_base_url}/{}", cover.filename_large));

    let base = normalized_base_url(public_base_url);
    let canonical_url = if is_preview {
        String::new()
    } else {
        format!("{base}/c/{}", est.slug)
    };
    let robots_meta = if is_preview {
        "noindex, nofollow"
    } else if is_demo {
        "noindex, follow"
    } else {
        "index, follow, max-image-preview:large, max-snippet:-1"
    };
    let meta_description = if !est.description.trim().is_empty() {
        est.description.clone()
    } else {
        format!("{} — {}", est.name, cat.t(locale, "app.tagline"))
    };
    let og_image = cover_url
        .as_deref()
        .or(logo_url.as_deref())
        .map(|rel| absolute_url(base, rel))
        .unwrap_or_else(|| og_image_url(base, locale));
    let json_ld = if is_preview || is_demo {
        String::new()
    } else {
        public_menu_json_ld(base, snapshot, cover_url.as_deref(), logo_url.as_deref())
    };

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
                    let (extras, removals) = sp
                        .choices
                        .iter()
                        .map(|choice| ChoiceView {
                            name: choice.name,
                            display_name: match locale {
                                Locale::PtBr => choice.name.0,
                                Locale::En => choice.name.1,
                                Locale::Es => choice.name.2,
                            },
                            price: choice
                                .price_centavos
                                .map(|price| (price, format_price(price, locale))),
                        })
                        .partition(|choice| choice.price.is_some());
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
                        removals,
                        extras,
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
        google_review_target: est
            .google_review_url
            .clone()
            .or_else(|| est.google_maps_url.clone()),
        google_review_heading: cat
            .t(locale, "public_menu.google_review_heading")
            .to_string(),
        google_review_body: cat.t(locale, "public_menu.google_review_body").to_string(),
        google_review_action: cat
            .t(locale, "public_menu.google_review_action")
            .to_string(),
        theme_css_href: theme_css_href.to_string(),
        is_preview,
        skip_label: cat.t(locale, "common.skip_to_content").to_string(),
        switcher_label: cat.t(locale, "lang.switcher_label").to_string(),
        preview_banner_text: cat.t(locale, "public_menu.preview_banner").to_string(),
        search_placeholder: cat.t(locale, "public_menu.search_placeholder").to_string(),
        categories_label: cat.t(locale, "public_menu.categories_label").to_string(),
        all_categories_text: cat.t(locale, "public_menu.all_categories").to_string(),
        unavailable_text: cat.t(locale, "public_menu.unavailable_badge").to_string(),
        choices_text: cat.t(locale, "public_menu.product_choices").into(),
        removable_text: cat.t(locale, "public_menu.removable_ingredients").into(),
        extras_text: cat.t(locale, "public_menu.extras").into(),
        empty_menu_text: cat.t(locale, "public_menu.empty_menu").to_string(),
        no_results_text: cat.t(locale, "public_menu.no_results").to_string(),
        powered_by_text: cat.t(locale, "public_menu.powered_by").to_string(),
        categories,
        menu_i18n_json: cat.public_menu_embedded_json().to_string(),
        robots_meta,
        canonical_url,
        meta_description,
        og_image_url: og_image,
        json_ld,
    };

    let html = tpl
        .render()
        .map_err(|e| AppError::Internal(format!("public menu render failed: {e}")))?;
    Ok(super::assets::version_static_assets(html))
}
