use axum::Json;
use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use chrono::Utc;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{AUTHORIZE_PATH, CONSENT_PATH, TOKEN_PATH, connections};
use crate::app::AppState;
use crate::application::{
    auth::login,
    mcp::oauth::{self, AuthorizeParams, TokenForm},
};
use crate::domain::mcp::{OAuthError, READ, READ_WRITE, WRITE, is_url_token, provider_name};
use crate::error::{AppError, AppResult};
use crate::http::middleware::{
    auth::{ClientIp, OptionalAuth, TenantContext, build_session_cookie, verify_csrf},
    locale::RequestLocale,
};
use crate::http::responses::take_flash;
use crate::infrastructure::database::queries::{
    find_establishment_for_account, is_establishment_owner,
    mcp_oauth::{self as queries, Authorization},
};
use crate::infrastructure::security::{constant_time_eq, generate_token_32, sha256_hex};
use crate::rendering::{
    PageMeta,
    mcp::{ConsentTemplate, ReturnTemplate},
    render_html,
};

const BROWSER_COOKIE: &str = "qr_mcp_browser";

pub async fn protected_resource(State(state): State<AppState>) -> Json<Value> {
    let origin = &state.config.public_base_url;
    Json(
        json!({"resource": oauth::resource(origin), "resource_name": "QR Cardápio",
        "authorization_servers": [origin], "scopes_supported": [READ, WRITE], "bearer_methods_supported": ["header"]}),
    )
}

pub async fn authorization_server(State(state): State<AppState>) -> Json<Value> {
    let origin = &state.config.public_base_url;
    Json(
        json!({"issuer": origin, "authorization_endpoint": format!("{origin}{AUTHORIZE_PATH}"),
        "token_endpoint": format!("{origin}{TOKEN_PATH}"), "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"], "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"], "scopes_supported": [READ, WRITE],
        "client_id_metadata_document_supported": true, "authorization_response_iss_parameter_supported": true}),
    )
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Internal(error) => {
                tracing::error!(%error, "MCP OAuth persistence failure");
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::InvalidClient => StatusCode::BAD_REQUEST,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, Json(json!({"error": self.to_string()}))).into_response()
    }
}

pub async fn authorize(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(params): Query<AuthorizeParams>,
) -> Result<Response, OAuthError> {
    if state.rate_limiters.mcp_oauth_ip.check_key(&ip).is_err() {
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"temporarily_unavailable"})),
        )
            .into_response());
    }
    let flow = params.validate(&state.config.public_base_url)?;
    let browser = browser_token(&jar)
        .map(str::to_owned)
        .map_or_else(generate_token_32, Ok)?;
    let id = Uuid::now_v7().to_string();
    queries::save_authorization(&state.pool, &id, &sha256_hex(&browser), &flow).await?;
    let cookie = Cookie::build((BROWSER_COOKIE, browser))
        .path("/mcp/oauth")
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Lax)
        .build();
    Ok((jar.add(cookie), consent_redirect(&id)).into_response())
}

fn browser_token(jar: &CookieJar) -> Option<&str> {
    jar.get(BROWSER_COOKIE)
        .map(Cookie::value)
        .filter(|value| is_url_token(value, 43, 43))
}

fn consent_redirect(id: &str) -> Redirect {
    Redirect::to(&format!("{CONSENT_PATH}?flow={id}"))
}

async fn load_flow(state: &AppState, jar: &CookieJar, id: &str) -> AppResult<Authorization> {
    let browser = browser_token(jar).ok_or(AppError::Validation("mcp.expired"))?;
    let id = Uuid::parse_str(id).map_err(|_| AppError::Validation("mcp.expired"))?;
    let flow = queries::load_authorization(
        &state.pool,
        &id.to_string(),
        &sha256_hex(browser),
        Utc::now().timestamp(),
    )
    .await?;
    flow.filter(|flow| flow.resource == oauth::resource(&state.config.public_base_url))
        .ok_or(AppError::Validation("mcp.expired"))
}

#[derive(Deserialize)]
pub struct FlowQuery {
    flow: String,
}

pub async fn google_start(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(query): Query<FlowQuery>,
) -> AppResult<Response> {
    let id = Uuid::parse_str(&query.flow)
        .map_err(|_| AppError::Validation("mcp.expired"))?
        .to_string();
    load_flow(&state, &jar, &id).await?;
    super::super::google::start_redirect(
        &state,
        ip,
        jar,
        crate::application::google::Intent::Mcp(&id),
    )
    .await
}

pub async fn consent(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
    Query(query): Query<FlowQuery>,
) -> AppResult<Response> {
    let flow = load_flow(&state, &jar, &query.flow).await?;
    let (jar, flash) = take_flash(jar);
    let page = PageMeta::new(locale, "mcp.authorize_heading", flash);
    let Some(auth) = auth else {
        let response = login_response(
            &flow,
            &query.flow,
            browser_token(&jar).unwrap_or_default(),
            page,
            "",
            None,
            (state.google.is_some(), StatusCode::OK),
        )?;
        return Ok((jar, response).into_response());
    };
    let Some(est) = find_establishment_for_account(&state.pool, &auth.account.id).await? else {
        let id = Uuid::parse_str(&query.flow).map_err(|_| AppError::Validation("mcp.expired"))?;
        return Ok(Redirect::to(&format!("/onboarding?mcp={id}")).into_response());
    };
    if !is_establishment_owner(&state.pool, &est.id, &auth.account.id).await? {
        return Err(AppError::Forbidden("mcp.owner_required"));
    }
    let callback =
        Url::parse(&flow.redirect_uri).map_err(|_| AppError::Validation("mcp.expired"))?;
    let view = ConsentTemplate {
        page,
        flow: &query.flow,
        provider: provider_name(&flow.client_id).ok_or(AppError::Validation("mcp.expired"))?,
        establishment: Some(&est.name),
        callback_host: callback
            .host_str()
            .ok_or(AppError::Validation("mcp.expired"))?,
        read_write: flow.scope == READ_WRITE,
        csrf: &auth.csrf_token,
        google_enabled: false,
        email: "",
        error: None,
    };
    Ok((jar, render_html(&view, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct LoginForm {
    flow: String,
    _csrf: String,
    email: String,
    password: String,
}

pub async fn login_submit(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let flow = load_flow(&state, &jar, &form.flow).await?;
    let browser = browser_token(&jar).ok_or(AppError::Validation("mcp.expired"))?;
    if !constant_time_eq(browser, &form._csrf) {
        return Err(AppError::Forbidden("error.invalid_csrf"));
    }
    let page = PageMeta::new(locale, "mcp.authorize_heading", None);
    if state.rate_limiters.login_ip.check_key(&ip).is_err() {
        return login_response(
            &flow,
            &form.flow,
            browser,
            page,
            &form.email,
            Some("error.rate_limited"),
            (state.google.is_some(), StatusCode::TOO_MANY_REQUESTS),
        );
    }
    match login(&state.pool, &state.passwords, &form.email, &form.password).await {
        Ok(auth) => Ok((
            jar.add(build_session_cookie(
                auth.raw_session_token,
                state.config.cookie_secure,
            )),
            consent_redirect(&form.flow),
        )
            .into_response()),
        Err(error) if error.status_code().is_client_error() => login_response(
            &flow,
            &form.flow,
            browser,
            page,
            &form.email,
            Some(error.i18n_key()),
            (state.google.is_some(), StatusCode::UNAUTHORIZED),
        ),
        Err(error) => Err(error),
    }
}

fn login_response(
    flow: &Authorization,
    id: &str,
    csrf: &str,
    page: PageMeta,
    email: &str,
    error: Option<&str>,
    (google_enabled, status): (bool, StatusCode),
) -> AppResult<Response> {
    render_html(
        &ConsentTemplate {
            page,
            flow: id,
            provider: provider_name(&flow.client_id).ok_or(AppError::Validation("mcp.expired"))?,
            establishment: None,
            callback_host: "",
            read_write: flow.scope == READ_WRITE,
            csrf,
            google_enabled,
            email,
            error,
        },
        status,
    )
}

#[derive(Deserialize)]
pub struct ConsentForm {
    flow: String,
    _csrf: String,
    decision: String,
}

pub async fn consent_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
    Form(form): Form<ConsentForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    connections::require_owner(&state, &tenant).await?;
    load_flow(&state, &jar, &form.flow).await?;
    let browser = browser_token(&jar).ok_or(AppError::Validation("mcp.expired"))?;
    let code = match form.decision.as_str() {
        "allow" => Some(generate_token_32()?),
        "deny" => None,
        _ => return Err(AppError::Validation("mcp.expired")),
    };
    let hash = code.as_deref().map(sha256_hex);
    let flow = queries::finish_authorization(
        &state.pool,
        &form.flow,
        &sha256_hex(browser),
        &connections::owner(&tenant),
        hash.as_deref(),
        Utc::now().timestamp(),
    )
    .await?
    .ok_or(AppError::Validation("mcp.expired"))?;
    let allowed = code.is_some();
    let mut callback =
        Url::parse(&flow.redirect_uri).map_err(|_| AppError::Validation("mcp.expired"))?;
    {
        let mut params = callback.query_pairs_mut();
        if let Some(code) = code {
            params.append_pair("code", &code);
        } else {
            params.append_pair("error", "access_denied");
        }
        if let Some(state) = flow.state {
            params.append_pair("state", &state);
        }
        params.append_pair("iss", &state.config.public_base_url);
    }
    render_html(
        &ReturnTemplate {
            page: PageMeta::new(
                locale,
                if allowed {
                    "mcp.authorized"
                } else {
                    "mcp.denied"
                },
                None,
            ),
            provider: provider_name(&flow.client_id).ok_or(AppError::Validation("mcp.expired"))?,
            callback: callback.to_string(),
        },
        StatusCode::OK,
    )
}

pub async fn token(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    form: Result<Form<TokenForm>, axum::extract::rejection::FormRejection>,
) -> Response {
    if state.rate_limiters.mcp_oauth_ip.check_key(&ip).is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"temporarily_unavailable"})),
        )
            .into_response();
    }
    let Ok(Form(form)) = form else {
        return OAuthError::InvalidRequest.into_response();
    };
    match oauth::token(&state.pool, form, &state.config.public_base_url).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => error.into_response(),
    }
}
