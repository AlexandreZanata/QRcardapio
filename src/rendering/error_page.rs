use askama::Template;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{PageMeta, render_html};
use crate::i18n::Locale;

#[derive(Template)]
#[template(path = "errors/status.html")]
struct ErrorTemplate {
    page: PageMeta,
    status: u16,
    body_key: &'static str,
    artwork: &'static str,
    action_path: &'static str,
    action_key: &'static str,
}

pub fn error_page(locale: Locale, status: StatusCode, detail: Option<&'static str>) -> Response {
    let (title_key, default_body, artwork, action_path, action_key) = match status {
        StatusCode::NOT_FOUND => (
            "status.missing_title",
            "status.missing_body",
            "missing",
            "/",
            "status.home",
        ),
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => (
            "status.forbidden_title",
            "error.forbidden",
            "forbidden",
            "/dashboard",
            "status.dashboard",
        ),
        code if code.is_server_error() => (
            "status.error_title",
            "error.internal",
            "error",
            "/",
            "status.home",
        ),
        _ => (
            "status.request_title",
            "status.request_body",
            "error",
            "/",
            "status.home",
        ),
    };
    let template = ErrorTemplate {
        page: PageMeta::new(locale, title_key, None),
        status: status.as_u16(),
        body_key: detail.unwrap_or(default_body),
        artwork,
        action_path,
        action_key,
    };
    let response = render_html(&template, status).unwrap_or_else(|_| status.into_response());
    ([(header::CACHE_CONTROL, "no-store")], response).into_response()
}
