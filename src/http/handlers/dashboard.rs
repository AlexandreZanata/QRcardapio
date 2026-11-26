use axum::Form;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::entitlements::{Denial, authorize, can_publish_menu};
use crate::application::publishing::{publish_establishment, unpublish_establishment};
use crate::domain::account::AccountRole;
use crate::domain::billing::billing_now;
use crate::domain::entitlements::Operation;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::database::queries::{
    list_categories_for_est, list_products_for_est, load_menu_preview,
};
use crate::rendering::dashboard::OverviewTemplate;
use crate::rendering::public_menu::{generate_brand_theme_css, render_public_menu_html};
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct CsrfForm {
    pub _csrf: String,
}

pub async fn overview_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let categories = list_categories_for_est(&state.pool, &est.id).await?;
    let products = list_products_for_est(&state.pool, &est.id).await?;
    let can_publish = can_publish_menu(&state.pool, state.billing.enforcement(), &est.id).await?;
    let (jar, flash) = take_flash(jar);

    let tpl = OverviewTemplate {
        page: PageMeta::new(locale, "dashboard.page_title", flash),
        brand_name: est.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "overview",
        csrf_token: tenant.auth.csrf_token,
        slug: est.slug.clone(),
        public_url: state.config.public_menu_url(&est.slug),
        is_published: est.is_published,
        has_unpublished_changes: est.has_unpublished_changes,
        published_version: est.published_version,
        last_published_at: est.last_published_at.clone(),
        category_count: categories.len(),
        product_count: products.len(),
        billing_enabled: state.billing.enabled(),
        can_publish,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn dashboard_theme_css(tenant: TenantContext) -> Response {
    let (_name, bytes) = generate_brand_theme_css(&tenant.establishment.accent_color);
    let mut res = (StatusCode::OK, bytes).into_response();
    let headers = res.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

pub async fn preview_menu_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
) -> AppResult<Response> {
    let snapshot = load_menu_preview(&state.pool, &tenant.establishment.id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let html = render_public_menu_html(
        &snapshot,
        locale,
        true,
        "/dashboard/media",
        "/dashboard/theme.css",
        &state.config.public_base_url,
    )?;
    Ok((StatusCode::OK, Html(html)).into_response())
}

pub async fn serve_tenant_media(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(filename): Path<String>,
) -> AppResult<Response> {
    let path = state
        .assets
        .asset_file_path(&tenant.establishment.id, &filename)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::NotFound("error.not_found"))?;

    let content_type = if filename.ends_with(".png") {
        "image/png"
    } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        return Err(AppError::NotFound("error.not_found"));
    };

    let mut res = (StatusCode::OK, bytes).into_response();
    let headers = res.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    Ok(res)
}

pub async fn publish_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;

    if let Err(denial) = authorize_operation(&state, &tenant, Operation::PublishMenu).await? {
        let target = match denial {
            Denial::NotEntitled(_) => "/dashboard/billing",
            _ => "/dashboard",
        };
        return Ok(with_flash_redirect(
            jar,
            target,
            true,
            denial.i18n_key(),
            state.config.cookie_secure,
        ));
    }

    match publish_establishment(
        &state.pool,
        &state.config,
        &state.publish_locks,
        &tenant.establishment.id,
    )
    .await
    {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            "/dashboard",
            false,
            "flash.published",
            state.config.cookie_secure,
        )),
        Err(_) => Ok(with_flash_redirect(
            jar,
            "/dashboard",
            true,
            "error.publish_failed",
            state.config.cookie_secure,
        )),
    }
}

pub async fn unpublish_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    if let Err(denial) = authorize_operation(&state, &tenant, Operation::UnpublishMenu).await? {
        return Err(denial.into_error());
    }
    unpublish_establishment(
        &state.pool,
        &state.config,
        &state.publish_locks,
        &tenant.establishment,
    )
    .await?;

    Ok(with_flash_redirect(
        jar,
        "/dashboard",
        false,
        "flash.unpublished",
        state.config.cookie_secure,
    ))
}

async fn authorize_operation(
    state: &AppState,
    tenant: &TenantContext,
    operation: Operation,
) -> AppResult<Result<(), Denial>> {
    Ok(authorize(
        &state.pool,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        operation,
        billing_now().date_naive(),
    )
    .await?
    .map(|_| ()))
}
