pub mod pages;
pub mod tools;

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH, VARY};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use sha2::{Digest, Sha256};

use crate::app::AppState;
use crate::error::AppResult;
use crate::infrastructure::database::queries::list_published_menus_for_sitemap;
use crate::seo::sitemap::{
    PublishedMenuEntry, build_llms_txt, build_manifest_json, build_robots_txt, build_sitemap_xml,
};

pub use pages::*;
pub use tools::*;

const SITEMAP_MENU_LIMIT: i64 = 45_000;

pub async fn robots_txt(State(state): State<AppState>) -> Response {
    let body = build_robots_txt(&state.config.public_base_url);
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "text/plain; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn llms_txt(State(state): State<AppState>) -> Response {
    let body = build_llms_txt(&state.config.public_base_url);
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "text/plain; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn manifest_webmanifest() -> Response {
    let body = build_manifest_json();
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "application/manifest+json; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=86400"),
        ],
        body,
    )
        .into_response()
}

pub async fn favicon_ico(State(state): State<AppState>) -> Response {
    serve_icon_file(&state, "favicon.ico", "image/x-icon").await
}

pub async fn apple_touch_icon(State(state): State<AppState>) -> Response {
    serve_icon_file(&state, "apple-touch-icon.png", "image/png").await
}

async fn serve_icon_file(state: &AppState, filename: &str, content_type: &'static str) -> Response {
    let path = state.config.static_dir.join("icons").join(filename);
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (CONTENT_TYPE, content_type),
                (CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn sitemap_xml(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let rows = list_published_menus_for_sitemap(&state.pool, SITEMAP_MENU_LIMIT).await?;
    let entries: Vec<PublishedMenuEntry> = rows
        .into_iter()
        .map(|row| PublishedMenuEntry {
            slug: row.slug,
            lastmod: row.lastmod,
        })
        .collect();

    let xml = build_sitemap_xml(&state.config.public_base_url, &entries);
    let digest = Sha256::digest(xml.as_bytes());
    let etag = format!("\"{}\"", &hex::encode(digest)[..16]);
    if let Some(inm) = headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok())
        && (inm == etag || inm == "*")
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let mut res = (StatusCode::OK, xml).into_response();
    let res_headers = res.headers_mut();
    res_headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    res_headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=600, stale-while-revalidate=3600"),
    );
    if let Ok(hv) = HeaderValue::from_str(&etag) {
        res_headers.insert(ETAG, hv);
    }
    Ok(res)
}

pub async fn redirect_en_slash() -> Redirect {
    Redirect::permanent("/en")
}

pub async fn redirect_es_slash() -> Redirect {
    Redirect::permanent("/es")
}

pub fn with_public_cache(mut res: Response, is_logged_in: bool) -> AppResult<Response> {
    let headers = res.headers_mut();
    if is_logged_in {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    } else {
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=300, stale-while-revalidate=3600"),
        );
    }
    headers.insert(VARY, HeaderValue::from_static("Cookie"));
    Ok(res)
}
