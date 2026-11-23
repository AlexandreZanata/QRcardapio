use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::app::AppState;
use crate::application::auth::authenticate_session;
use crate::domain::account::{Account, AccountRole};
use crate::domain::establishment::Establishment;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::infrastructure::security::constant_time_eq;

pub const SESSION_COOKIE_NAME: &str = "qr_session";

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub account: Account,
    pub session_id: String,
    pub csrf_token: String,
}

#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<AuthContext>);

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub auth: AuthContext,
    pub establishment: Establishment,
}

#[derive(Debug, Clone)]
pub struct AdminContext {
    pub auth: AuthContext,
}

pub enum AuthRejection {
    RedirectLogin,
    RedirectOnboarding,
    Status(StatusCode),
}

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        match self {
            Self::RedirectLogin => Redirect::to("/login").into_response(),
            Self::RedirectOnboarding => Redirect::to("/onboarding").into_response(),
            Self::Status(code) => code.into_response(),
        }
    }
}

impl FromRequestParts<AppState> for OptionalAuth {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let Some(cookie) = jar.get(SESSION_COOKIE_NAME) else {
            return Ok(Self(None));
        };
        match authenticate_session(&state.pool, cookie.value()).await {
            Ok(Some((sess, account))) => Ok(Self(Some(AuthContext {
                account,
                session_id: sess.id,
                csrf_token: sess.csrf_token,
            }))),
            _ => Ok(Self(None)),
        }
    }
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let OptionalAuth(maybe) = OptionalAuth::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRejection::RedirectLogin)?;
        maybe.ok_or(AuthRejection::RedirectLogin)
    }
}

impl FromRequestParts<AppState> for TenantContext {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthContext::from_request_parts(parts, state).await?;
        let est = find_establishment_for_account(&state.pool, &auth.account.id)
            .await
            .map_err(|_| AuthRejection::Status(StatusCode::INTERNAL_SERVER_ERROR))?;
        match est {
            Some(establishment) => Ok(Self {
                auth,
                establishment,
            }),
            None => Err(AuthRejection::RedirectOnboarding),
        }
    }
}

impl FromRequestParts<AppState> for AdminContext {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthContext::from_request_parts(parts, state).await?;
        if auth.account.role != AccountRole::Admin {
            return Err(AuthRejection::Status(StatusCode::FORBIDDEN));
        }
        Ok(Self { auth })
    }
}

pub fn verify_csrf(auth: &AuthContext, submitted: &str) -> AppResult<()> {
    if submitted.is_empty() || !constant_time_eq(&auth.csrf_token, submitted) {
        return Err(AppError::Forbidden("error.invalid_csrf"));
    }
    Ok(())
}

pub fn build_session_cookie(raw_token: String, secure: bool) -> Cookie<'static> {
    let secure_attr = if secure { "; Secure" } else { "" };
    let header = format!(
        "{SESSION_COOKIE_NAME}={raw_token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800{secure_attr}"
    );
    Cookie::parse(header).unwrap_or_else(|_| {
        Cookie::build((SESSION_COOKIE_NAME, raw_token))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(secure)
            .build()
    })
}

pub fn build_clear_session_cookie(secure: bool) -> Cookie<'static> {
    let mut c = Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();
    c.make_removal();
    c
}

#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let connect_info = parts.extensions.get::<ConnectInfo<SocketAddr>>().copied();
        Ok(Self(extract_client_ip(
            &parts.headers,
            connect_info,
            state.config.trust_proxy,
        )))
    }
}

pub fn extract_client_ip(
    headers: &HeaderMap,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    trust_proxy: bool,
) -> IpAddr {
    if trust_proxy
        && let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = xff.split(',').next()
        && let Ok(ip) = first.trim().parse::<IpAddr>()
    {
        return ip;
    }
    connect_info
        .map(|ci| ci.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}
