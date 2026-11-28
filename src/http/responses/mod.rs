use axum::Json;
use axum::http::header::{ACCEPT, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Serialize;

use crate::error::AppError;
use crate::i18n::{Catalog, Locale};
use crate::rendering::FlashMessage;

pub(crate) mod public_html;

const FLASH_COOKIE_NAME: &str = "qr_flash";

#[derive(Serialize)]
struct DialogError<'a> {
    key: &'static str,
    message: &'a str,
}

pub fn prefers_dialog_response(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .is_some_and(|value| value == "application/json")
}

pub fn dialog_error(catalog: &Catalog, locale: Locale, error: &AppError) -> Response {
    let key = error.i18n_key();
    (
        error.status_code(),
        Json(DialogError {
            key,
            message: catalog.t(locale, key),
        }),
    )
        .into_response()
}

pub fn dialog_saved(jar: CookieJar, key: &str, secure: bool) -> Response {
    (
        with_flash_cookie(jar, false, key, secure),
        StatusCode::NO_CONTENT,
    )
        .into_response()
}

pub fn with_flash_redirect(
    jar: CookieJar,
    to: &str,
    is_error: bool,
    key: &str,
    secure: bool,
) -> Response {
    (
        with_flash_cookie(jar, is_error, key, secure),
        Redirect::to(to),
    )
        .into_response()
}

pub(crate) fn with_flash_cookie(
    jar: CookieJar,
    is_error: bool,
    key: &str,
    secure: bool,
) -> CookieJar {
    let prefix = if is_error { "err:" } else { "ok:" };
    let val = format!("{prefix}{key}");
    let cookie = Cookie::build((FLASH_COOKIE_NAME, val))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();
    jar.add(cookie)
}

pub fn take_flash(jar: CookieJar) -> (CookieJar, Option<FlashMessage>) {
    let Some(c) = jar.get(FLASH_COOKIE_NAME) else {
        return (jar, None);
    };
    let raw = c.value().to_string();
    let msg = if let Some(key) = raw.strip_prefix("err:") {
        Some(FlashMessage {
            is_error: true,
            key: key.to_string(),
        })
    } else {
        raw.strip_prefix("ok:").map(|key| FlashMessage {
            is_error: false,
            key: key.to_string(),
        })
    };

    let mut removal = Cookie::build((FLASH_COOKIE_NAME, "")).path("/").build();
    removal.make_removal();
    (jar.add(removal), msg)
}

pub fn check_etag_not_modified(headers: &HeaderMap, etag_value: &str) -> Option<Response> {
    if let Some(inm) = headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok())
        && (inm.trim() == etag_value || inm.trim() == "*")
    {
        let mut res = StatusCode::NOT_MODIFIED.into_response();
        if let Ok(hv) = HeaderValue::from_str(etag_value) {
            res.headers_mut().insert(ETAG, hv);
        }
        return Some(res);
    }
    None
}
