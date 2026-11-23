use axum::extract::State;
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;

use crate::app::AppState;
use crate::domain::account::AccountRole;
use crate::error::AppResult;
use crate::http::middleware::auth::TenantContext;
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::take_flash;
use crate::infrastructure::database::queries::find_media_asset_for_est;
use crate::qr::{generate_inline_svg, generate_png, generate_svg};
use crate::rendering::qr::{QrPrintTemplate, QrTemplate};
use crate::rendering::{PageMeta, render_html};

pub async fn qr_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let public_url = state.config.public_menu_url(&est.slug);
    let qr_svg = generate_inline_svg(&public_url)?;
    let (jar, flash) = take_flash(jar);

    let tpl = QrTemplate {
        page: PageMeta::new(locale, "qr.page_title", flash),
        brand_name: est.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "qr",
        csrf_token: tenant.auth.csrf_token,
        is_published: est.is_published,
        public_url,
        qr_svg,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn qr_print_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let public_url = state.config.public_menu_url(&est.slug);
    let qr_svg = generate_inline_svg(&public_url)?;

    let logo_url = match &est.logo_asset_id {
        Some(id) => find_media_asset_for_est(&state.pool, &est.id, id)
            .await?
            .map(|m| format!("/dashboard/media/{}", m.filename_thumb)),
        None => None,
    };

    let tpl = QrPrintTemplate {
        page: PageMeta::new(locale, "print.page_title", None),
        est_name: est.name.clone(),
        public_url,
        logo_url,
        qr_svg,
    };
    render_html(&tpl, StatusCode::OK)
}

pub async fn download_qr_svg(
    State(state): State<AppState>,
    tenant: TenantContext,
) -> AppResult<Response> {
    let public_url = state.config.public_menu_url(&tenant.establishment.slug);
    let svg = generate_svg(&public_url)?;
    let mut res = (StatusCode::OK, svg).into_response();
    let headers = res.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    let disp = format!(
        "attachment; filename=\"qrcode-{}.svg\"",
        tenant.establishment.slug
    );
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        headers.insert(CONTENT_DISPOSITION, hv);
    }
    Ok(res)
}

pub async fn download_qr_png(
    State(state): State<AppState>,
    tenant: TenantContext,
) -> AppResult<Response> {
    let public_url = state.config.public_menu_url(&tenant.establishment.slug);
    let png = generate_png(&public_url)?;
    let mut res = (StatusCode::OK, png).into_response();
    let headers = res.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
    let disp = format!(
        "attachment; filename=\"qrcode-{}.png\"",
        tenant.establishment.slug
    );
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        headers.insert(CONTENT_DISPOSITION, hv);
    }
    Ok(res)
}
