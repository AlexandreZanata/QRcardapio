use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::app::AppState;
use crate::i18n::Locale;
use crate::infrastructure::database::query_sqlite_version;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct ReadyResponse {
    status: &'static str,
    sqlite_version: String,
    i18n_version: String,
}

pub async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn readyz(State(state): State<AppState>) -> Response {
    match query_sqlite_version(&state.pool).await {
        Ok(version) => (
            StatusCode::OK,
            Json(ReadyResponse {
                status: "ready",
                sqlite_version: version,
                i18n_version: state.catalog.version_hash().to_string(),
            }),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(error = %err, "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse { status: "unready" }),
            )
                .into_response()
        }
    }
}

/// Serves embedded i18n JSON dictionaries (`/static/i18n/{locale}.json`) from memory
/// with immutable cache headers and ETag validation for zero-reload frontend switching.
pub async fn serve_i18n_catalog(
    State(state): State<AppState>,
    Path(file): Path<String>,
    headers: HeaderMap,
) -> Response {
    let raw_locale = file.strip_suffix(".json").unwrap_or(&file);
    let Some(locale) = Locale::parse(raw_locale) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let etag_val = format!("\"{}-{}\"", state.catalog.version_hash(), locale.code());
    if let Some(inm) = headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok())
        && (inm == etag_val || inm == "*")
    {
        return StatusCode::NOT_MODIFIED.into_response();
    }

    let body = state.catalog.raw_json(locale);
    let mut res = (StatusCode::OK, body).into_response();
    let res_headers = res.headers_mut();
    res_headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    if let Ok(etag) = HeaderValue::from_str(&etag_val) {
        res_headers.insert(ETAG, etag);
    }
    res_headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    res
}
