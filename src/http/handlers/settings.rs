use axum::Form;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::establishments::{
    UpdateEstablishmentInput, remove_logo, update_profile, upload_logo,
};
use crate::domain::account::AccountRole;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::assets::MAX_LOGO_UPLOAD_BYTES;
use crate::infrastructure::database::queries::find_media_asset_for_est;
use crate::rendering::dashboard::SettingsTemplate;
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct SettingsForm {
    pub _csrf: String,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub description_en: String,
    #[serde(default)]
    pub description_es: String,
    pub accent_color: String,
    pub google_maps_url: Option<String>,
    pub google_review_url: Option<String>,
}

#[derive(Deserialize)]
pub struct CsrfForm {
    pub _csrf: String,
}

async fn resolve_logo_url(state: &AppState, tenant: &TenantContext) -> AppResult<Option<String>> {
    match &tenant.establishment.logo_asset_id {
        Some(id) => Ok(
            find_media_asset_for_est(&state.pool, &tenant.establishment.id, id)
                .await?
                .map(|m| format!("/dashboard/media/{}", m.filename_thumb)),
        ),
        None => Ok(None),
    }
}

pub async fn settings_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let logo_url = resolve_logo_url(&state, &tenant).await?;
    let (jar, flash) = take_flash(jar);

    let tpl = SettingsTemplate {
        google_enabled: state.google.is_some(),
        mcp_enabled: state.config.mcp_enabled,
        page: PageMeta::new(locale, "settings.page_title", flash),
        brand_name: est.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "settings",
        csrf_token: tenant.auth.csrf_token,
        base_host: state.config.public_base_url.clone(),
        name_value: est.name.clone(),
        slug_value: est.slug.clone(),
        slug_locked: est.published_version > 0,
        description_value: est.description.clone(),
        description_en: est.description_en.clone(),
        description_es: est.description_es.clone(),
        accent_color: est.accent_color.clone(),
        google_maps_url: est.google_maps_url.clone().unwrap_or_default(),
        google_review_url: est.google_review_url.clone().unwrap_or_default(),
        logo_url,
        error_key: None,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn settings_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<SettingsForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;

    match update_profile(
        &state.pool,
        &state.publish_locks,
        &tenant.establishment,
        UpdateEstablishmentInput {
            name: &form.name,
            slug: &form.slug,
            description: &form.description,
            description_en: &form.description_en,
            description_es: &form.description_es,
            accent_color: &form.accent_color,
            google_maps_url: form.google_maps_url.as_deref(),
            google_review_url: form.google_review_url.as_deref(),
        },
    )
    .await
    {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            "/dashboard/settings",
            false,
            "flash.settings_saved",
            state.config.cookie_secure,
        )),
        Err(AppError::Validation("error.slug_locked")) => Ok(with_flash_redirect(
            jar,
            "/dashboard/settings",
            true,
            "error.slug_locked",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let logo_url = resolve_logo_url(&state, &tenant).await?;
            let tpl = SettingsTemplate {
                google_enabled: state.google.is_some(),
                mcp_enabled: state.config.mcp_enabled,
                page: PageMeta::new(locale, "settings.page_title", None),
                brand_name: tenant.establishment.name.clone(),
                has_establishment: true,
                has_theme: true,
                is_admin: tenant.auth.account.role == AccountRole::Admin,
                active_nav: "settings",
                csrf_token: tenant.auth.csrf_token,
                base_host: state.config.public_base_url.clone(),
                name_value: form.name,
                slug_value: form.slug,
                slug_locked: tenant.establishment.published_version > 0,
                description_value: form.description,
                description_en: form.description_en,
                description_es: form.description_es,
                accent_color: form.accent_color,
                google_maps_url: form
                    .google_maps_url
                    .or_else(|| tenant.establishment.google_maps_url.clone())
                    .unwrap_or_default(),
                google_review_url: form
                    .google_review_url
                    .or_else(|| tenant.establishment.google_review_url.clone())
                    .unwrap_or_default(),
                logo_url,
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}

pub async fn upload_logo_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut csrf = String::new();
    let mut logo_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::Validation("error.invalid_image"))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "logo" {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::Validation("error.image_too_large"))?;
            if bytes.len() > MAX_LOGO_UPLOAD_BYTES {
                return Ok(with_flash_redirect(
                    jar,
                    "/dashboard/settings",
                    true,
                    "error.image_too_large",
                    state.config.cookie_secure,
                ));
            }
            logo_bytes = Some(bytes.to_vec());
        } else if name == "_csrf" {
            csrf = field.text().await.unwrap_or_default();
        }
    }

    verify_csrf(&tenant.auth, &csrf)?;
    if state
        .rate_limiters
        .upload_account
        .check_key(&tenant.auth.account.id)
        .is_err()
    {
        return Err(AppError::RateLimited);
    }

    let bytes = logo_bytes.unwrap_or_default();
    match upload_logo(&state.pool, &state.assets, &tenant.establishment, bytes).await {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            "/dashboard/settings",
            false,
            "flash.logo_updated",
            state.config.cookie_secure,
        )),
        Err(err) => Ok(with_flash_redirect(
            jar,
            "/dashboard/settings",
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
    }
}

pub async fn remove_logo_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    remove_logo(&state.pool, &state.assets, &tenant.establishment).await?;
    Ok(with_flash_redirect(
        jar,
        "/dashboard/settings",
        false,
        "flash.logo_removed",
        state.config.cookie_secure,
    ))
}
