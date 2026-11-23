use std::net::IpAddr;
use std::os::unix::fs::MetadataExt;

use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::domain::establishment::Slug;
use crate::error::AppResult;
use crate::http::middleware::auth::OptionalAuth;
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::check_etag_not_modified;
use crate::infrastructure::storage::resolve_safe_subpath;
use crate::rendering::error_page::error_page;
use crate::rendering::landing::LandingTemplate;
use crate::rendering::{PageMeta, render_html};

pub async fn landing(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    let billing_enabled = state.asaas.is_some();
    let tpl = LandingTemplate {
        page: PageMeta::new(locale, "landing.page_title", None),
        is_logged_in: auth.is_some(),
        billing_enabled,
        show_plan: billing_enabled || local_plan_preview(&state.config.public_base_url),
        plans: crate::rendering::billing::PlanOffer::catalog(),
    };
    render_html(&tpl, StatusCode::OK)
}

fn local_plan_preview(public_base_url: &str) -> bool {
    let Ok(base) = reqwest::Url::parse(public_base_url) else {
        return false;
    };
    base.scheme() == "http"
        && base.host_str().is_some_and(|host| {
            host == "localhost"
                || match host.trim_matches(['[', ']']).parse::<IpAddr>() {
                    Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
                    Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
                    Err(_) => false,
                }
        })
}

/// Serves a published static menu (`/c/{slug}`) directly from disk WITHOUT touching SQLite.
pub async fn serve_published_menu(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    serve_published_file_from_disk(
        &state.config.published_dir(),
        &slug,
        "index.html",
        locale,
        &headers,
    )
    .await
}

/// Serves static assets inside a published menu directory (`/c/{slug}/{*file}`) WITHOUT touching SQLite.
pub async fn serve_published_asset(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path((slug, file)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    serve_published_file_from_disk(
        &state.config.published_dir(),
        &slug,
        &file,
        locale,
        &headers,
    )
    .await
}

/// Pure filesystem delivery handler that takes only the `published_dir` root (no database handle).
pub async fn serve_published_file_from_disk(
    published_dir: &std::path::Path,
    slug: &str,
    relative_file: &str,
    locale: crate::i18n::Locale,
    headers: &HeaderMap,
) -> Response {
    let Ok(clean_slug) = Slug::validate_existing(slug) else {
        return render_public_404(locale);
    };
    let Ok(est_dir) = resolve_safe_subpath(published_dir, clean_slug.as_str()) else {
        return render_public_404(locale);
    };
    let Ok(target) = resolve_safe_subpath(&est_dir, relative_file) else {
        return render_public_404(locale);
    };

    let Ok(meta) = tokio::fs::metadata(&target).await else {
        return render_public_404(locale);
    };
    if !meta.is_file() {
        return render_public_404(locale);
    }

    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let etag = format!("\"{}-{}-{}\"", meta.ino(), meta.size(), mtime_ns);
    if let Some(not_mod) = check_etag_not_modified(headers, &etag) {
        return not_mod;
    }

    let Ok(bytes) = tokio::fs::read(&target).await else {
        return render_public_404(locale);
    };

    let (content_type, cache_control) = match target.extension().and_then(|e| e.to_str()) {
        Some("html") => (
            "text/html; charset=utf-8",
            "public, max-age=60, stale-while-revalidate=300",
        ),
        Some("css") => (
            "text/css; charset=utf-8",
            "public, max-age=31536000, immutable",
        ),
        Some("jpg") | Some("jpeg") => ("image/jpeg", "public, max-age=31536000, immutable"),
        Some("png") => ("image/png", "public, max-age=31536000, immutable"),
        _ => ("application/octet-stream", "public, max-age=3600"),
    };

    let mut res = (StatusCode::OK, bytes).into_response();
    let res_headers = res.headers_mut();
    res_headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    res_headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache_control));
    if let Ok(hv) = HeaderValue::from_str(&etag) {
        res_headers.insert(ETAG, hv);
    }
    res
}

fn render_public_404(locale: crate::i18n::Locale) -> Response {
    error_page(
        locale,
        StatusCode::NOT_FOUND,
        Some("public_menu.not_found_body"),
    )
}
