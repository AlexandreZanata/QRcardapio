use axum::http::header::{ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::rendering::FlashMessage;

const FLASH_COOKIE_NAME: &str = "qr_flash";

pub fn with_flash_redirect(
    jar: CookieJar,
    to: &str,
    is_error: bool,
    key: &str,
    secure: bool,
) -> Response {
    let prefix = if is_error { "err:" } else { "ok:" };
    let val = format!("{prefix}{key}");
    let cookie = Cookie::build((FLASH_COOKIE_NAME, val))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();
    (jar.add(cookie), Redirect::to(to)).into_response()
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
