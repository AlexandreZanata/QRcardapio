use std::io::{Error, ErrorKind};

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, LOCATION};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::domain::establishment::Slug;
use crate::error::{AppError, AppResult};
use crate::http::responses::check_etag_not_modified;
use crate::http::responses::public_html::{HtmlKind, storefront_etag};
use crate::infrastructure::storage::resolve_safe_subpath;

pub async fn redirect_trailing_slash(Path(slug): Path<String>) -> AppResult<Response> {
    let valid =
        Slug::validate_existing(&slug).map_err(|_| AppError::NotFound("error.not_found"))?;
    let target = format!("/s/{}/", valid.as_str());
    Ok((StatusCode::MOVED_PERMANENTLY, [(LOCATION, target)]).into_response())
}

pub async fn storefront_index(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let valid =
        Slug::validate_existing(&slug).map_err(|_| AppError::NotFound("error.not_found"))?;
    serve_storefront_file(&state, valid.as_str(), "index.html", &headers).await
}

pub async fn storefront_asset(
    State(state): State<AppState>,
    Path((slug, file)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let valid =
        Slug::validate_existing(&slug).map_err(|_| AppError::NotFound("error.not_found"))?;
    let trimmed = file.trim_start_matches('/');
    let rel = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    serve_storefront_file(&state, valid.as_str(), rel, &headers).await
}

async fn serve_storefront_file(
    state: &AppState,
    slug: &str,
    relative: &str,
    headers: &HeaderMap,
) -> AppResult<Response> {
    if slug.starts_with('.') {
        return Err(AppError::NotFound("error.not_found"));
    }
    let slug_dir = resolve_safe_subpath(&state.config.storefronts_dir(), slug)?;
    let path = resolve_safe_subpath(&slug_dir, relative)?;
    let (content_type, cache_control, sw_scope) = asset_headers(slug, relative)?;
    let mut response = if relative == "index.html" {
        let page = state
            .public_html
            .read(path, HtmlKind::Storefront)
            .await
            .map_err(storefront_file_error)?;
        storefront_response(headers, page.body.clone(), &page.etag)
    } else {
        let (bytes, etag) = tokio::task::spawn_blocking(move || {
            if !slug_dir.join("index.html").try_exists()? {
                return Err(AppError::NotFound("delivery.storefront_not_found"));
            }
            let bytes = std::fs::read(path).map_err(storefront_file_error)?;
            let etag = storefront_etag(&bytes);
            Ok((bytes, etag))
        })
        .await
        .map_err(|err| AppError::Internal(format!("storefront read failed: {err}")))??;
        storefront_response(headers, bytes.into(), &etag)
    };

    let res_headers = response.headers_mut();
    res_headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    res_headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache_control));
    if let Some(scope) = sw_scope
        && let Ok(scope_val) = HeaderValue::from_str(&scope)
    {
        res_headers.insert(HeaderName::from_static("service-worker-allowed"), scope_val);
    }
    Ok(response)
}

fn storefront_file_error(error: Error) -> AppError {
    match error.kind() {
        ErrorKind::NotFound => AppError::NotFound("delivery.storefront_not_found"),
        _ => AppError::Io(error),
    }
}

fn storefront_response(headers: &HeaderMap, bytes: Bytes, etag: &str) -> Response {
    if let Some(response) = check_etag_not_modified(headers, etag) {
        return response;
    }
    let mut response = Response::new(Body::from(bytes));
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(ETAG, value);
    }
    response
}

fn asset_headers(
    slug: &str,
    relative: &str,
) -> AppResult<(&'static str, &'static str, Option<String>)> {
    if relative == "index.html" {
        return Ok((
            "text/html; charset=utf-8",
            "public, max-age=60, stale-while-revalidate=300",
            None,
        ));
    }
    if relative == "sw.js" {
        return Ok((
            "application/javascript; charset=utf-8",
            "no-cache",
            Some(format!("/s/{slug}/")),
        ));
    }
    if relative == "manifest.webmanifest" {
        return Ok((
            "application/manifest+json; charset=utf-8",
            "public, max-age=300",
            None,
        ));
    }
    if relative
        .strip_prefix("catalog.v")
        .and_then(|rest| rest.strip_suffix(".json"))
        .and_then(|version| version.parse::<u64>().ok())
        .is_some_and(|version| version > 0)
    {
        return Ok((
            "application/json; charset=utf-8",
            "public, max-age=31536000, immutable",
            None,
        ));
    }
    if relative.starts_with("theme-") && relative.ends_with(".css") && !relative.contains('/') {
        return Ok((
            "text/css; charset=utf-8",
            "public, max-age=31536000, immutable",
            None,
        ));
    }
    if matches!(relative, "icon-192.png" | "icon-512.png") {
        return Ok(("image/png", "public, max-age=300", None));
    }
    if let Some(rest) = relative.strip_prefix("media/")
        && !rest.is_empty()
        && !rest.contains('/')
    {
        let content_type = if rest.ends_with(".webp") {
            "image/webp"
        } else if rest.ends_with(".png") {
            "image/png"
        } else if rest.ends_with(".jpg") || rest.ends_with(".jpeg") {
            "image/jpeg"
        } else if rest.ends_with(".avif") {
            "image/avif"
        } else {
            return Err(AppError::NotFound("error.not_found"));
        };
        return Ok((content_type, "public, max-age=31536000, immutable", None));
    }
    Err(AppError::NotFound("error.not_found"))
}
