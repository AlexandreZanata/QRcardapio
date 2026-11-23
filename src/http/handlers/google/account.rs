use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;

use super::{ACCOUNT_PATH, begin};
use crate::app::AppState;
use crate::application::google::Intent;
use crate::error::{AppError, AppResult};
use crate::http::handlers::dashboard::CsrfForm;
use crate::http::middleware::{
    auth::{AuthContext, ClientIp, verify_csrf},
    locale::RequestLocale,
};
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::database::queries::google;
use crate::rendering::{
    PageMeta,
    google::{GoogleAccountTemplate, GoogleContinueTemplate},
    render_html,
};

pub(super) async fn page(
    State(state): State<AppState>,
    auth: AuthContext,
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
) -> AppResult<Response> {
    let linked = google::has_identity(&state.pool, &auth.account.id).await?;
    let (jar, flash) = take_flash(jar);
    let page = GoogleAccountTemplate {
        page: PageMeta::new(locale, "google.account", flash),
        email: auth.account.email,
        csrf: auth.csrf_token,
        linked,
        has_password: !auth.account.password_hash.is_empty(),
    };
    Ok((jar, render_html(&page, StatusCode::OK)?).into_response())
}

pub(super) async fn link(
    State(state): State<AppState>,
    auth: AuthContext,
    ClientIp(ip): ClientIp,
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    verify_csrf(&auth, &form._csrf)?;
    if google::has_identity(&state.pool, &auth.account.id).await? {
        return Err(AppError::Conflict("google.link_conflict"));
    }
    let (jar, url) = begin(&state, ip, jar, Intent::Link(&auth.session_id)).await?;
    let view = GoogleContinueTemplate {
        page: PageMeta::new(locale, "google.link", None),
        url: url.as_str(),
    };
    Ok((jar, render_html(&view, StatusCode::OK)?).into_response())
}

pub(super) async fn unlink(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    verify_csrf(&auth, &form._csrf)?;
    if !google::unlink_identity(&state.pool, &auth.account.id).await? {
        return Err(AppError::Conflict("google.unlink_requires_password"));
    }
    Ok(with_flash_redirect(
        jar,
        ACCOUNT_PATH,
        false,
        "google.unlinked",
        state.config.cookie_secure,
    ))
}
