use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Form, Json};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::outbox;
use crate::error::AppResult;
use crate::http::middleware::auth::{AdminContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::database::queries::{
    event_dispatch_status, find_establishment_for_account,
};
use crate::rendering::admin::AdminOutboxTemplate;
use crate::rendering::{PageMeta, render_html};

pub const RECOVERY_PATH: &str = "/admin/outbox/recovery";

pub async fn outbox(State(state): State<AppState>, _admin: AdminContext) -> AppResult<Response> {
    let status = event_dispatch_status::status(&state.pool, Utc::now()).await?;
    Ok(Json(status).into_response())
}

pub async fn recovery_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    admin: AdminContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let status = event_dispatch_status::status(&state.pool, Utc::now()).await?;
    let has_establishment = find_establishment_for_account(&state.pool, &admin.auth.account.id)
        .await?
        .is_some();
    let (jar, flash) = take_flash(jar);
    let template = AdminOutboxTemplate {
        page: PageMeta::new(locale, "admin_outbox.title", flash),
        brand_name: "QR Cardápio",
        has_establishment,
        has_theme: false,
        is_admin: true,
        active_nav: "admin",
        csrf_token: admin.auth.csrf_token,
        dead_letters: status.dead_letters,
    };
    Ok((jar, render_html(&template, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct ReplayForm {
    _csrf: String,
    consumer: String,
    event_id: String,
    generation: i64,
}

pub async fn replay_submit(
    State(state): State<AppState>,
    admin: AdminContext,
    jar: CookieJar,
    Form(form): Form<ReplayForm>,
) -> AppResult<Response> {
    verify_csrf(&admin.auth, &form._csrf)?;
    outbox::schedule_replay(
        &state.pool,
        &admin.auth.account.id,
        &form.consumer,
        &form.event_id,
        form.generation,
        Utc::now(),
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        RECOVERY_PATH,
        false,
        "admin_outbox.replay_scheduled",
        state.config.cookie_secure,
    ))
}

pub async fn private_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    response
}
