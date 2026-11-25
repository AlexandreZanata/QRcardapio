mod account;

#[cfg(test)]
mod tests;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::middleware;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use reqwest::Url;
use serde::Deserialize;

use super::affiliates::{build_clear_referral_cookie, referral_cookie_value};
use super::onboarding::PlanIntent;
use crate::app::AppState;
use crate::application::{
    affiliates::attribute_new_account,
    auth::issue_session,
    google::{self, Intent},
};
use crate::config::GOOGLE_CALLBACK_PATH;
use crate::error::{AppError, AppResult};
use crate::http::middleware::{
    auth::{ClientIp, OptionalAuth, build_session_cookie},
    locale::RequestLocale,
    security_headers::no_store,
};
use crate::http::responses::with_flash_redirect;
use crate::infrastructure::database::queries::{
    find_establishment_for_account,
    google::{self as queries, PendingGoogleFlow},
};
use crate::infrastructure::security::generate_token_32;

const BROWSER_COOKIE: &str = "qr_google_flow";
const ACCOUNT_PATH: &str = "/account/security";

pub fn routes(enabled: bool) -> Router<AppState> {
    if !enabled {
        return Router::new();
    }
    Router::new()
        .route("/auth/google/start", get(start))
        .route(GOOGLE_CALLBACK_PATH, get(callback))
        .route("/auth/google/link", post(account::link))
        .route(ACCOUNT_PATH, get(account::page))
        .route("/account/google/unlink", post(account::unlink))
        .layer(DefaultBodyLimit::max(8 * 1024))
        .layer(middleware::from_fn(no_store))
}

async fn start(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
) -> AppResult<Response> {
    let suffix = plan.suffix(&state);
    if auth.is_some() {
        let path = if suffix.is_empty() {
            "/dashboard".to_owned()
        } else {
            format!("/onboarding{suffix}")
        };
        return Ok(Redirect::to(&path).into_response());
    }
    let intent = plan
        .selected()
        .filter(|_| state.billing.enabled())
        .map_or(Intent::SignIn, Intent::Plan);
    start_redirect(&state, ip, jar, intent).await
}

pub async fn start_redirect(
    state: &AppState,
    ip: std::net::IpAddr,
    jar: CookieJar,
    intent: Intent<'_>,
) -> AppResult<Response> {
    let (jar, url) = begin(state, ip, jar, intent).await?;
    Ok((jar, Redirect::to(url.as_str())).into_response())
}

async fn begin(
    state: &AppState,
    ip: std::net::IpAddr,
    jar: CookieJar,
    intent: Intent<'_>,
) -> AppResult<(CookieJar, Url)> {
    let provider = state
        .google
        .as_ref()
        .ok_or(AppError::NotFound("error.not_found"))?;
    if state.rate_limiters.google_ip.check_key(&ip).is_err() {
        return Err(AppError::RateLimited);
    }
    let browser = browser_token(&jar)
        .map(str::to_owned)
        .map_or_else(generate_token_32, Ok)?;
    let url = google::begin(&state.pool, provider, &browser, intent).await?;
    let cookie = Cookie::build((BROWSER_COOKIE, browser))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.cookie_secure)
        .build();
    Ok((jar.add(cookie), url))
}

#[derive(Deserialize)]
struct CallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    OptionalAuth(auth): OptionalAuth,
    jar: CookieJar,
    query: Result<Query<CallbackQuery>, axum::extract::rejection::QueryRejection>,
) -> AppResult<Response> {
    let provider = state
        .google
        .as_ref()
        .ok_or(AppError::NotFound("error.not_found"))?;
    let (Ok(Query(query)), Some(browser)) = (query, browser_token(&jar)) else {
        return Ok(error(&state, jar, "/login", "google.expired"));
    };
    let Some(raw_state) = query.state.as_deref().filter(|value| opaque_token(value)) else {
        return Ok(error(&state, jar, "/login", "google.expired"));
    };
    let Some(flow) = google::take(&state.pool, provider, raw_state, browser).await? else {
        return Ok(error(&state, jar, "/login", "google.expired"));
    };
    let failure = return_path(&flow, true);
    let linking = flow.purpose == "link";
    if query.error.is_some() {
        let key = if query.error.as_deref() == Some("access_denied") {
            "google.cancelled"
        } else {
            "google.unavailable"
        };
        return Ok(error(&state, jar, &failure, key));
    }
    let link_account = if linking {
        let Some(auth) = auth
            .as_ref()
            .filter(|auth| flow.local_session_id.as_deref() == Some(auth.session_id.as_str()))
        else {
            return Ok(error(&state, jar, &failure, "google.link_mismatch"));
        };
        Some(auth)
    } else {
        // A password login or another successful Google flow may have changed this browser's session.
        // Preserve the active account instead of switching it through a callback from another tab.
        if auth.is_some() {
            return Ok(Redirect::to(&return_path(&flow, false)).into_response());
        }
        None
    };
    let Some(code) = query.code.as_deref().filter(|code| {
        !code.is_empty() && code.len() <= 2048 && code.bytes().all(|byte| byte.is_ascii_graphic())
    }) else {
        return Ok(error(&state, jar, &failure, "google.invalid_identity"));
    };
    let identity = match provider
        .exchange(code, &flow.code_verifier, &flow.nonce_hash)
        .await
    {
        Ok(identity) => identity,
        Err(failure_error) => return Ok(error(&state, jar, &failure, failure_error.i18n_key())),
    };
    if let Some(auth) = link_account {
        let linked = queries::link_identity(
            &state.pool,
            &auth.account.id,
            &auth.session_id,
            &identity.subject,
            &identity.email,
        )
        .await;
        return match linked {
            Ok(()) => Ok(with_flash_redirect(
                jar,
                ACCOUNT_PATH,
                false,
                "google.linked",
                state.config.cookie_secure,
            )),
            Err(err) if err.status_code().is_client_error() => {
                Ok(error(&state, jar, &failure, err.i18n_key()))
            }
            Err(err) => Err(err),
        };
    }
    let (account, created) =
        match queries::resolve_account(&state.pool, &identity.subject, &identity.email, locale)
            .await
        {
            Ok(result) => result,
            Err(err) if err.status_code().is_client_error() => {
                return Ok(error(&state, jar, &failure, err.i18n_key()));
            }
            Err(err) => return Err(err),
        };
    let ref_code = if created {
        referral_cookie_value(&jar).map(str::to_owned)
    } else {
        None
    };
    if created
        && let Err(err) = attribute_new_account(
            &state.pool,
            ref_code.as_deref(),
            &account.id,
            &account.email,
        )
        .await
    {
        tracing::warn!(error = %err, "affiliate attribution failed during google signup");
    }
    let destination = if flow.purpose == "signin"
        && find_establishment_for_account(&state.pool, &account.id)
            .await?
            .is_none()
    {
        "/onboarding".to_owned()
    } else {
        return_path(&flow, false)
    };
    let session = issue_session(&state.pool, account).await?;
    let jar = if ref_code.is_some() {
        jar.add(build_clear_referral_cookie(state.config.cookie_secure))
    } else {
        jar
    };
    let jar = jar.add(build_session_cookie(
        session.raw_session_token,
        state.config.cookie_secure,
    ));
    Ok(with_flash_redirect(
        jar,
        &destination,
        false,
        if created {
            "flash.registered"
        } else {
            "flash.logged_in"
        },
        state.config.cookie_secure,
    ))
}

fn return_path(flow: &PendingGoogleFlow, failed: bool) -> String {
    match flow.purpose.as_str() {
        "link" => ACCOUNT_PATH.to_owned(),
        "plan" => {
            let path = if failed { "/login" } else { "/onboarding" };
            flow.plan_key
                .as_ref()
                .map_or_else(|| path.to_owned(), |plan| format!("{path}?plan={plan}"))
        }
        "mcp" => flow
            .mcp_flow_id
            .as_deref()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .map_or_else(
                || "/login".to_owned(),
                |id| format!("/mcp/oauth/consent?flow={id}"),
            ),
        _ if failed => "/login".into(),
        _ => "/dashboard".into(),
    }
}

fn error(state: &AppState, jar: CookieJar, path: &str, key: &str) -> Response {
    with_flash_redirect(jar, path, true, key, state.config.cookie_secure)
}

fn browser_token(jar: &CookieJar) -> Option<&str> {
    jar.get(BROWSER_COOKIE)
        .map(Cookie::value)
        .filter(|value| opaque_token(value))
}

fn opaque_token(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
