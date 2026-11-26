use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::delivery::access::staff_access;
use crate::application::delivery::{publishing, settings as delivery_settings};
use crate::domain::account::AccountRole;
use crate::domain::delivery::coupon::{CouponInput, CouponKind};
use crate::domain::delivery::storefront::StorefrontSettingsInput;
use crate::domain::delivery::zone::DeliveryZoneInput;
use crate::domain::product::parse_price_centavos;
use crate::error::{AppError, AppResult};
use crate::http::handlers::categories::CsrfOnlyForm;
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::rendering::delivery::DeliverySettingsTemplate;
use crate::rendering::{PageMeta, render_html};

const SETTINGS_PATH: &str = "/dashboard/delivery/settings";

#[derive(Deserialize)]
pub struct StorefrontSettingsForm {
    pub _csrf: String,
    #[serde(default)]
    pub accept_cash: Option<String>,
    #[serde(default)]
    pub accept_card_on_delivery: Option<String>,
    #[serde(default)]
    pub accept_pix_on_delivery: Option<String>,
    #[serde(default)]
    pub pix_key_display: String,
    #[serde(default)]
    pub contact_whatsapp: String,
    #[serde(default)]
    pub pickup_instructions: String,
    #[serde(default)]
    pub default_prep_minutes: i64,
}

#[derive(Deserialize)]
pub struct ZoneForm {
    pub _csrf: String,
    pub neighborhood: String,
    pub city: String,
    pub fee: String,
    #[serde(default)]
    pub min_order: String,
    #[serde(default)]
    pub free_delivery_from: String,
    pub eta_min_minutes: i64,
    pub eta_max_minutes: i64,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub is_active: Option<String>,
}

#[derive(Deserialize)]
pub struct CouponForm {
    pub _csrf: String,
    pub code: String,
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub min_subtotal: String,
    #[serde(default)]
    pub valid_from: String,
    #[serde(default)]
    pub valid_until: String,
    #[serde(default)]
    pub max_redemptions: String,
    #[serde(default)]
    pub max_per_customer: String,
    #[serde(default)]
    pub allow_delivery: Option<String>,
    #[serde(default)]
    pub allow_pickup: Option<String>,
    #[serde(default)]
    pub is_active: Option<String>,
}

#[derive(Deserialize)]
pub struct CustomerBlockForm {
    pub _csrf: String,
    pub phone: String,
    pub block: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub return_to: String,
}

pub async fn settings_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let access = staff_access(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &est.id,
        tenant.role,
    )
    .await?;
    if !access.storefront_management {
        return Err(AppError::Forbidden("error.module_unavailable"));
    }
    let view = delivery_settings::load_settings_view(&state.pool, &est.id).await?;
    let (jar, flash) = take_flash(jar);
    let tpl = DeliverySettingsTemplate {
        page: PageMeta::new(locale, "delivery.settings.page_title", flash),
        brand_name: est.name.clone(),
        csrf_token: tenant.auth.csrf_token,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        has_establishment: true,
        has_theme: true,
        active_nav: "delivery_settings",
        slug: est.slug.clone(),
        storefront_url: state.config.public_storefront_url(&est.slug),
        storefront: view.storefront,
        commercial: view.commercial,
        zones: view.zones,
        coupons: view.coupons,
        can_edit: access.settings,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn save_settings_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<StorefrontSettingsForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    delivery_settings::save_settings(
        &state.pool,
        &state.config,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &state.publish_locks,
        &tenant.establishment.id,
        tenant.role,
        StorefrontSettingsInput {
            accept_cash: form.accept_cash.is_some(),
            accept_card_on_delivery: form.accept_card_on_delivery.is_some(),
            accept_pix_on_delivery: form.accept_pix_on_delivery.is_some(),
            pix_key_display: &form.pix_key_display,
            contact_whatsapp: &form.contact_whatsapp,
            pickup_instructions: &form.pickup_instructions,
            default_prep_minutes: form.default_prep_minutes,
        },
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.settings_saved",
        state.config.cookie_secure,
    ))
}

pub async fn create_zone_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<ZoneForm>,
) -> AppResult<Response> {
    save_zone_inner(&state, &tenant, jar, None, form).await
}

pub async fn update_zone_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    jar: CookieJar,
    Form(form): Form<ZoneForm>,
) -> AppResult<Response> {
    save_zone_inner(&state, &tenant, jar, Some(&id), form).await
}

async fn save_zone_inner(
    state: &AppState,
    tenant: &TenantContext,
    jar: CookieJar,
    zone_id: Option<&str>,
    form: ZoneForm,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let fee_centavos = parse_price_centavos(&form.fee)?;
    let min_order_centavos = parse_optional_price(&form.min_order)?;
    let free_delivery_from_centavos = parse_optional_price(&form.free_delivery_from)?;

    delivery_settings::save_zone(
        &state.pool,
        &state.config,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &state.publish_locks,
        &tenant.establishment.id,
        tenant.role,
        zone_id,
        DeliveryZoneInput {
            neighborhood: &form.neighborhood,
            city: &form.city,
            fee_centavos,
            min_order_centavos,
            free_delivery_from_centavos,
            eta_min_minutes: form.eta_min_minutes,
            eta_max_minutes: form.eta_max_minutes,
            is_active: form.is_active.is_some(),
            position: form.position,
        },
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.zone_saved",
        state.config.cookie_secure,
    ))
}

pub async fn delete_zone_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    jar: CookieJar,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    delivery_settings::delete_zone(
        &state.pool,
        &state.config,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &state.publish_locks,
        &tenant.establishment.id,
        tenant.role,
        &id,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.zone_deleted",
        state.config.cookie_secure,
    ))
}

pub async fn create_coupon_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CouponForm>,
) -> AppResult<Response> {
    save_coupon_inner(&state, &tenant, jar, None, form).await
}

pub async fn update_coupon_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    jar: CookieJar,
    Form(form): Form<CouponForm>,
) -> AppResult<Response> {
    save_coupon_inner(&state, &tenant, jar, Some(&id), form).await
}

async fn save_coupon_inner(
    state: &AppState,
    tenant: &TenantContext,
    jar: CookieJar,
    coupon_id: Option<&str>,
    form: CouponForm,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let kind = CouponKind::parse(form.kind.trim())
        .ok_or(AppError::Validation("delivery.invalid_coupon"))?;
    let value = parse_price_centavos(&form.value)
        .map_err(|_| AppError::Validation("delivery.invalid_coupon"))?;
    let min_subtotal_centavos = parse_optional_price(&form.min_subtotal)?.unwrap_or(0);
    let valid_from = parse_optional_datetime(&form.valid_from)?;
    let valid_until = parse_optional_datetime(&form.valid_until)?;
    let max_redemptions = parse_optional_i64(&form.max_redemptions)?;
    let max_per_customer = parse_optional_i64(&form.max_per_customer)?;

    delivery_settings::save_coupon(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        coupon_id,
        CouponInput {
            code: &form.code,
            kind,
            value,
            min_subtotal_centavos,
            valid_from,
            valid_until,
            max_redemptions,
            max_per_customer,
            allow_delivery: form.allow_delivery.is_some(),
            allow_pickup: form.allow_pickup.is_some(),
            is_active: form.is_active.is_some(),
        },
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.coupon_saved",
        state.config.cookie_secure,
    ))
}

pub async fn delete_coupon_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    jar: CookieJar,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    delivery_settings::delete_coupon(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        &id,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.coupon_deleted",
        state.config.cookie_secure,
    ))
}

pub async fn customer_block_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CustomerBlockForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let block = matches!(form.block.trim(), "1" | "true" | "block" | "on");
    delivery_settings::set_customer_block(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        &tenant.auth.account.id,
        &form.phone,
        block,
        &form.reason,
    )
    .await?;
    let target = match form.return_to.trim() {
        "/dashboard/delivery/orders" => "/dashboard/delivery/orders",
        _ => SETTINGS_PATH,
    };
    Ok(with_flash_redirect(
        jar,
        target,
        false,
        "delivery.flash.customer_block_updated",
        state.config.cookie_secure,
    ))
}

pub async fn publish_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    publishing::publish_storefront(
        &state.pool,
        &state.config,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &state.publish_locks,
        &tenant.establishment.id,
        tenant.role,
        publishing::PublicationKind::Manual,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.published",
        state.config.cookie_secure,
    ))
}

pub async fn unpublish_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    publishing::unpublish_storefront(
        &state.pool,
        &state.config,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &state.publish_locks,
        &tenant.establishment.id,
        tenant.role,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        SETTINGS_PATH,
        false,
        "delivery.flash.unpublished",
        state.config.cookie_secure,
    ))
}

fn parse_optional_price(raw: &str) -> AppResult<Option<i64>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parse_price_centavos(trimmed)?))
    }
}

fn parse_optional_i64(raw: &str) -> AppResult<Option<i64>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        trimmed
            .parse::<i64>()
            .map(Some)
            .map_err(|_| AppError::Validation("delivery.invalid_coupon"))
    }
}

fn parse_optional_datetime(raw: &str) -> AppResult<Option<NaiveDateTime>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S"))
        .map(Some)
        .map_err(|_| AppError::Validation("delivery.invalid_coupon"))
}
