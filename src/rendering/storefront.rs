use askama::Template;
use serde::{Deserialize, Serialize};

use crate::domain::commerce::business_hours::Schedule;
use crate::domain::commerce::catalog_snapshot::CatalogContent;
use crate::domain::delivery::storefront::StorefrontSettings;
use crate::domain::delivery::zone::DeliveryZone;
use crate::domain::establishment::{AccentColor, Establishment};
use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale, format_price};
use crate::seo::normalized_base_url;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorefrontCatalogJson {
    pub establishment_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub accent_color: String,
    pub catalog_version: i64,
    pub storefront_version: i64,
    pub logo_url: Option<String>,
    pub cover_url: Option<String>,
    pub categories: Vec<StorefrontCategoryJson>,
    pub zones: Vec<StorefrontZoneJson>,
    pub schedule: StorefrontScheduleJson,
    pub payments: StorefrontPaymentsJson,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorefrontCategoryJson {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub products: Vec<StorefrontProductJson>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorefrontProductJson {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub price_centavos: i64,
    pub is_available: bool,
    pub image_url: Option<String>,
    pub variations: Vec<crate::domain::commerce::catalog_options::ProductVariation>,
    pub addon_groups: Vec<crate::domain::commerce::catalog_snapshot::CatalogAddonGroup>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorefrontZoneJson {
    pub id: String,
    pub neighborhood: String,
    pub city: String,
    pub fee_centavos: i64,
    pub min_order_centavos: Option<i64>,
    pub free_delivery_from_centavos: Option<i64>,
    pub eta_min_minutes: i64,
    pub eta_max_minutes: i64,
}

impl From<&DeliveryZone> for StorefrontZoneJson {
    fn from(zone: &DeliveryZone) -> Self {
        Self {
            id: zone.id.clone(),
            neighborhood: zone.neighborhood.clone(),
            city: zone.city.clone(),
            fee_centavos: zone.fee_centavos,
            min_order_centavos: zone.min_order_centavos,
            free_delivery_from_centavos: zone.free_delivery_from_centavos,
            eta_min_minutes: zone.eta_min_minutes,
            eta_max_minutes: zone.eta_max_minutes,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorefrontScheduleJson {
    pub time_zone: String,
    pub delivery: bool,
    pub pickup: bool,
    pub minimum_centavos: i64,
    pub acceptance_minutes: i64,
    pub paused: bool,
    pub weekly: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorefrontPaymentsJson {
    pub accept_cash: bool,
    pub accept_card_on_delivery: bool,
    pub accept_pix_on_delivery: bool,
    pub pickup_instructions: String,
}

impl From<&Schedule> for StorefrontScheduleJson {
    fn from(schedule: &Schedule) -> Self {
        Self {
            time_zone: schedule.settings.time_zone.clone(),
            delivery: schedule.settings.delivery,
            pickup: schedule.settings.pickup,
            minimum_centavos: schedule.settings.minimum_centavos,
            acceptance_minutes: schedule.settings.acceptance_minutes,
            paused: schedule.settings.paused,
            weekly: schedule.weekly.iter().map(ToString::to_string).collect(),
        }
    }
}

impl From<&StorefrontSettings> for StorefrontPaymentsJson {
    fn from(settings: &StorefrontSettings) -> Self {
        Self {
            accept_cash: settings.accept_cash,
            accept_card_on_delivery: settings.accept_card_on_delivery,
            accept_pix_on_delivery: settings.accept_pix_on_delivery,
            pickup_instructions: settings.pickup_instructions.clone(),
        }
    }
}

pub struct StorefrontProductView {
    pub id: String,
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
    pub has_variations: bool,
    pub is_available: bool,
    pub image_url: Option<String>,
}

pub struct StorefrontCategoryView {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub display_name: String,
    pub products: Vec<StorefrontProductView>,
}

#[derive(Template)]
#[template(path = "storefront/index.html")]
pub struct StorefrontTemplate {
    pub locale_code: &'static str,
    pub establishment_name: String,
    pub establishment_description: String,
    pub establishment_description_en: String,
    pub establishment_description_es: String,
    pub establishment_display_description: String,
    pub slug: String,
    pub canonical_url: String,
    pub manifest_href: String,
    pub theme_css_href: String,
    pub theme_color: String,
    pub icon_href: String,
    pub logo_url: Option<String>,
    pub cover_url: Option<String>,
    pub delivery_enabled: bool,
    pub pickup_enabled: bool,
    pub switcher_label: String,
    pub skip_label: String,
    pub search_placeholder: String,
    pub categories_label: String,
    pub all_categories_text: String,
    pub unavailable_text: String,
    pub empty_menu_text: String,
    pub no_results_text: String,
    pub from_label: String,
    pub cart_open_text: String,
    pub privacy_notice_text: String,
    pub categories: Vec<StorefrontCategoryView>,
    pub storefront_data_json: String,
    pub storefront_i18n_json: String,
}

impl StorefrontTemplate {
    fn t(&self, key: &'static str) -> &'static str {
        Catalog::global().t(Locale::PtBr, key)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_storefront_catalog_json(
    est: &Establishment,
    settings: &StorefrontSettings,
    content: &CatalogContent,
    zones: &[DeliveryZone],
    schedule: &Schedule,
    logo_url: Option<String>,
    cover_url: Option<String>,
    product_images: &std::collections::HashMap<String, String>,
) -> StorefrontCatalogJson {
    StorefrontCatalogJson {
        establishment_id: est.id.clone(),
        slug: est.slug.clone(),
        name: est.name.clone(),
        description: est.description.clone(),
        description_en: est.description_en.clone(),
        description_es: est.description_es.clone(),
        accent_color: est.accent_color.clone(),
        catalog_version: 0,
        storefront_version: 0,
        logo_url,
        cover_url,
        categories: storefront_categories(content, product_images),
        zones: zones.iter().map(StorefrontZoneJson::from).collect(),
        schedule: schedule.into(),
        payments: settings.into(),
    }
}

pub(crate) fn storefront_categories(
    content: &CatalogContent,
    product_images: &std::collections::HashMap<String, String>,
) -> Vec<StorefrontCategoryJson> {
    content
        .categories
        .iter()
        .map(|cat| StorefrontCategoryJson {
            id: cat.id.clone(),
            name: cat.name.clone(),
            name_en: cat.name_en.clone(),
            name_es: cat.name_es.clone(),
            description: cat.description.clone(),
            description_en: cat.description_en.clone(),
            description_es: cat.description_es.clone(),
            products: cat
                .products
                .iter()
                .map(|prod| StorefrontProductJson {
                    id: prod.id.clone(),
                    name: prod.name.clone(),
                    name_en: prod.name_en.clone(),
                    name_es: prod.name_es.clone(),
                    description: prod.description.clone(),
                    description_en: prod.description_en.clone(),
                    description_es: prod.description_es.clone(),
                    price_centavos: prod.price_centavos,
                    is_available: prod.is_available,
                    image_url: product_images.get(&prod.id).cloned(),
                    variations: prod.variations.clone(),
                    addon_groups: prod.addon_groups.clone(),
                })
                .collect(),
        })
        .collect()
}

pub fn render_storefront_html(
    payload: &StorefrontCatalogJson,
    theme_css_href: &str,
    public_base_url: &str,
) -> AppResult<String> {
    let cat = Catalog::global();
    let locale = Locale::PtBr;
    let base = normalized_base_url(public_base_url);
    let slug = &payload.slug;
    let accent = AccentColor::parse(&payload.accent_color)
        .unwrap_or_else(|_| AccentColor::parse(AccentColor::DEFAULT_HEX).expect("valid default"));

    let categories = payload
        .categories
        .iter()
        .filter(|c| !c.products.is_empty())
        .map(|c| {
            let products = c
                .products
                .iter()
                .map(|p| {
                    let display_centavos = p
                        .variations
                        .iter()
                        .filter(|v| v.is_available)
                        .map(|v| v.price_centavos)
                        .min()
                        .unwrap_or(p.price_centavos);
                    StorefrontProductView {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        name_en: p.name_en.clone(),
                        name_es: p.name_es.clone(),
                        display_name: p.name.clone(),
                        description: p.description.clone(),
                        description_en: p.description_en.clone(),
                        description_es: p.description_es.clone(),
                        display_description: p.description.clone(),
                        price_centavos: display_centavos,
                        price_display: format_price(display_centavos, locale),
                        has_variations: p.variations.iter().any(|v| v.is_available),
                        is_available: p.is_available
                            && (p.variations.is_empty()
                                || p.variations.iter().any(|v| v.is_available))
                            && p.addon_groups.iter().all(|group| {
                                group
                                    .options
                                    .iter()
                                    .filter(|option| option.is_available)
                                    .count()
                                    >= group.min_select as usize
                            }),
                        image_url: p.image_url.clone(),
                    }
                })
                .collect();
            StorefrontCategoryView {
                id: c.id.clone(),
                name: c.name.clone(),
                name_en: c.name_en.clone(),
                name_es: c.name_es.clone(),
                display_name: c.name.clone(),
                products,
            }
        })
        .collect();

    let storefront_data_json = serde_json::to_string(payload)
        .map_err(|e| AppError::Internal(format!("storefront json failed: {e}")))?
        .replace("</", "<\\/");

    let privacy_notice_text = cat
        .t(locale, "storefront.privacy_notice")
        .replace("{establishment}", &payload.name);

    let tpl = StorefrontTemplate {
        locale_code: locale.code(),
        establishment_name: payload.name.clone(),
        establishment_description: payload.description.clone(),
        establishment_description_en: payload.description_en.clone(),
        establishment_description_es: payload.description_es.clone(),
        establishment_display_description: payload.description.clone(),
        slug: slug.clone(),
        canonical_url: format!("{base}/s/{slug}/"),
        manifest_href: format!("/s/{slug}/manifest.webmanifest"),
        theme_css_href: theme_css_href.to_owned(),
        theme_color: accent.as_str().to_owned(),
        icon_href: format!("/s/{slug}/icon-192.png?v={}", payload.storefront_version),
        logo_url: payload.logo_url.clone(),
        cover_url: payload.cover_url.clone(),
        delivery_enabled: payload.schedule.delivery,
        pickup_enabled: payload.schedule.pickup,
        switcher_label: cat.t(locale, "lang.switcher_label").to_owned(),
        skip_label: cat.t(locale, "common.skip_to_content").to_owned(),
        search_placeholder: cat.t(locale, "public_menu.search_placeholder").to_owned(),
        categories_label: cat.t(locale, "public_menu.categories_label").to_owned(),
        all_categories_text: cat.t(locale, "public_menu.all_categories").to_owned(),
        unavailable_text: cat.t(locale, "public_menu.unavailable_badge").to_owned(),
        empty_menu_text: cat.t(locale, "public_menu.empty_menu").to_owned(),
        no_results_text: cat.t(locale, "public_menu.no_results").to_owned(),
        from_label: cat.t(locale, "storefront.from_price").to_owned(),
        cart_open_text: cat.t(locale, "storefront.view_cart").to_owned(),
        privacy_notice_text,
        categories,
        storefront_data_json,
        storefront_i18n_json: cat.storefront_embedded_json().to_owned(),
    };

    let html = tpl
        .render()
        .map_err(|e| AppError::Internal(format!("storefront render failed: {e}")))?;
    Ok(super::assets::version_static_assets(html))
}
