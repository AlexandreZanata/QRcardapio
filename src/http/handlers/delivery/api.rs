use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::application::delivery::storefront_api::{
    self, StorefrontOrderInput, StorefrontOrderOutcome, StorefrontQuoteInput,
    StorefrontQuoteResponse,
};
use crate::domain::establishment::Slug;
use crate::error::AppError;
use crate::http::middleware::auth::ClientIp;
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::check_etag_not_modified;
use crate::i18n::{Catalog, Locale};

#[derive(Serialize)]
struct ApiErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

#[derive(Serialize)]
struct ApiRepricedBody<'a> {
    code: &'a str,
    message: &'a str,
    quote: &'a StorefrontQuoteResponse,
}

#[derive(Deserialize)]
pub struct CancelOrderBody {
    pub cancel_token: String,
}

pub async fn post_quote(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    Path(slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(err) = Slug::validate_existing(&slug) {
        return json_app_error(locale, &AppError::NotFound(err.i18n_key()));
    }
    if let Err(err) = enforce_same_origin(&headers, &state.config.public_base_url) {
        return json_app_error(locale, &err);
    }
    if state
        .rate_limiters
        .delivery_quote_ip
        .check_key(&ip)
        .is_err()
    {
        return json_app_error(locale, &AppError::RateLimited);
    }
    let Some(input) = parse_json_request::<StorefrontQuoteInput>(&headers, &body) else {
        return bad_request_json(locale, "delivery.invalid_checkout");
    };

    match storefront_api::quote(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &slug,
        input,
        Utc::now(),
    )
    .await
    {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(err) => json_app_error(locale, &err),
    }
}

pub async fn post_order(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    Path(slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if Slug::validate_existing(&slug).is_err() {
        return json_app_error(locale, &AppError::NotFound("delivery.storefront_not_found"));
    }
    if let Err(err) = enforce_same_origin(&headers, &state.config.public_base_url) {
        return json_app_error(locale, &err);
    }
    if state
        .rate_limiters
        .delivery_order_ip
        .check_key(&ip)
        .is_err()
    {
        return json_app_error(locale, &AppError::RateLimited);
    }
    let Some(idempotency_key) = parse_idempotency_key(&headers) else {
        return bad_request_json(locale, "delivery.invalid_checkout");
    };
    let Some(input) = parse_json_request::<StorefrontOrderInput>(&headers, &body) else {
        return bad_request_json(locale, "delivery.invalid_checkout");
    };

    match storefront_api::place_order(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &state.rate_limiters,
        &slug,
        idempotency_key,
        input,
        Utc::now(),
    )
    .await
    {
        Ok(StorefrontOrderOutcome::Placed(receipt)) => (
            StatusCode::CREATED,
            [(CACHE_CONTROL, "no-store")],
            Json(receipt),
        )
            .into_response(),
        Ok(StorefrontOrderOutcome::Repriced(quote)) => {
            let msg = Catalog::global().t(locale, "commerce.checkout_repriced");
            (
                StatusCode::CONFLICT,
                Json(ApiRepricedBody {
                    code: "commerce.checkout_repriced",
                    message: msg,
                    quote: &quote,
                }),
            )
                .into_response()
        }
        Err(err) => json_app_error(locale, &err),
    }
}

pub async fn get_order(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    Path((slug, order_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if Slug::validate_existing(&slug).is_err() {
        return json_app_error(locale, &AppError::NotFound("delivery.storefront_not_found"));
    }
    if state
        .rate_limiters
        .delivery_track_ip
        .check_key(&ip)
        .is_err()
    {
        return json_app_error(locale, &AppError::RateLimited);
    }

    match storefront_api::get_tracking(&state.pool, &slug, &order_id).await {
        Ok((view, etag)) => {
            let mut resp = check_etag_not_modified(&headers, &etag)
                .unwrap_or_else(|| (StatusCode::OK, Json(view)).into_response());
            if let Ok(val) = etag.parse() {
                resp.headers_mut().insert(ETAG, val);
            }
            resp.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
            resp
        }
        Err(err) => json_app_error(locale, &err),
    }
}

pub async fn cancel_order(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    Path((slug, order_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if Slug::validate_existing(&slug).is_err() {
        return json_app_error(locale, &AppError::NotFound("delivery.storefront_not_found"));
    }
    if let Err(err) = enforce_same_origin(&headers, &state.config.public_base_url) {
        return json_app_error(locale, &err);
    }
    if state
        .rate_limiters
        .delivery_cancel_ip
        .check_key(&ip)
        .is_err()
    {
        return json_app_error(locale, &AppError::RateLimited);
    }
    let Some(payload) = parse_json_request::<CancelOrderBody>(&headers, &body) else {
        return bad_request_json(locale, "delivery.invalid_checkout");
    };

    match storefront_api::cancel_order(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &slug,
        &order_id,
        &payload.cancel_token,
        Utc::now(),
    )
    .await
    {
        Ok(order) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "order_id": order.public_id,
                "state": order.state.as_str(),
                "version": order.version,
            })),
        )
            .into_response(),
        Err(err) => json_app_error(locale, &err),
    }
}

fn parse_json_request<T: for<'de> Deserialize<'de>>(headers: &HeaderMap, body: &[u8]) -> Option<T> {
    let ct_ok = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| ct.split(';').next())
        .is_some_and(|ct| ct.trim().eq_ignore_ascii_case("application/json"));
    if ct_ok {
        serde_json::from_slice(body).ok()
    } else {
        None
    }
}

fn parse_idempotency_key(headers: &HeaderMap) -> Option<Uuid> {
    let raw = headers.get("idempotency-key")?.to_str().ok()?.trim();
    let uuid = Uuid::parse_str(raw).ok()?;
    (uuid.get_version_num() == 4).then_some(uuid)
}

fn enforce_same_origin(headers: &HeaderMap, public_base_url: &str) -> Result<(), AppError> {
    if let Some(sfs) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
        && sfs.eq_ignore_ascii_case("cross-site")
    {
        return Err(AppError::Forbidden("error.forbidden"));
    }
    if let Some(origin) = headers.get(ORIGIN).and_then(|v| v.to_str().ok()) {
        let origin_clean = origin.trim().trim_end_matches('/');
        let base_origin = extract_origin(public_base_url);
        let host = headers
            .get(HOST)
            .and_then(|v| v.to_str().ok())
            .map(str::trim);
        let matches_base = base_origin.is_some_and(|b| b.eq_ignore_ascii_case(origin_clean));
        let matches_host = host.is_some_and(|h| {
            origin_clean.eq_ignore_ascii_case(&format!("http://{h}"))
                || origin_clean.eq_ignore_ascii_case(&format!("https://{h}"))
        });
        if !matches_base && !matches_host {
            return Err(AppError::Forbidden("error.forbidden"));
        }
    }
    Ok(())
}

fn extract_origin(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let (scheme, rest) = trimmed.split_once("://")?;
    let host_port = rest.split('/').next()?;
    if host_port.is_empty() {
        None
    } else {
        Some(format!("{scheme}://{host_port}"))
    }
}

fn bad_request_json(locale: Locale, code: &'static str) -> Response {
    let msg = Catalog::global().t(locale, code);
    (
        StatusCode::BAD_REQUEST,
        Json(ApiErrorBody { code, message: msg }),
    )
        .into_response()
}

fn json_app_error(locale: Locale, err: &AppError) -> Response {
    let code = err.i18n_key();
    let msg = Catalog::global().t(locale, code);
    (err.status_code(), Json(ApiErrorBody { code, message: msg })).into_response()
}
