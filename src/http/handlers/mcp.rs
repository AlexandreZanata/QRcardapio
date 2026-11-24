mod connections;
mod oauth;
mod protocol;
mod tools;

use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use crate::app::AppState;
use crate::domain::mcp::is_allowed_origin;
use crate::http::middleware::security_headers::no_store;

pub const AUTHORIZE_PATH: &str = "/mcp/oauth/authorize";
pub const CONSENT_PATH: &str = "/mcp/oauth/consent";
pub const TOKEN_PATH: &str = "/mcp/oauth/token";
pub const RESOURCE_METADATA: &str = "/.well-known/oauth-protected-resource/mcp";
pub const AUTH_METADATA: &str = "/.well-known/oauth-authorization-server";

pub fn routes(state: &AppState) -> Router<AppState> {
    if !state.config.mcp_enabled {
        return Router::new();
    }
    let machine_routes = Router::new()
        .route(
            "/mcp",
            post(protocol::call)
                .get(protocol::stream)
                .delete(protocol::stream)
                .options(preflight_ok),
        )
        .route(
            RESOURCE_METADATA,
            get(oauth::protected_resource).options(preflight_ok),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth::protected_resource).options(preflight_ok),
        )
        .route(
            AUTH_METADATA,
            get(oauth::authorization_server).options(preflight_ok),
        )
        .route(TOKEN_PATH, post(oauth::token).options(preflight_ok))
        .route_layer(middleware::from_fn_with_state(state.clone(), cors_for_mcp));

    let browser_routes = Router::new()
        .route(AUTHORIZE_PATH, get(oauth::authorize))
        .route(
            CONSENT_PATH,
            get(oauth::consent).post(oauth::consent_submit),
        )
        .route("/mcp/oauth/login", post(oauth::login_submit))
        .route("/mcp/oauth/google/start", get(oauth::google_start))
        .route("/dashboard/integrations", get(connections::page))
        .route("/dashboard/integrations/revoke", post(connections::revoke));

    machine_routes
        .merge(browser_routes)
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn(no_store))
}

async fn preflight_ok() -> Response {
    (
        StatusCode::NO_CONTENT,
        [(header::ALLOW, "GET, POST, DELETE, OPTIONS")],
    )
        .into_response()
}

async fn cors_for_mcp(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let origin_value = request.headers().get(header::ORIGIN).cloned();
    let allowed_origin = match origin_value {
        Some(raw) => {
            let permitted = raw
                .to_str()
                .is_ok_and(|origin| is_allowed_origin(origin, &state.config.public_base_url));
            if !permitted {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32600, "message": "Origin not permitted" }
                    })),
                )
                    .into_response();
            }
            Some(raw)
        }
        None => None,
    };

    let is_options = request.method() == Method::OPTIONS;
    let mut response = next.run(request).await;
    if let Some(origin) = allowed_origin {
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
        headers.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("WWW-Authenticate, MCP-Protocol-Version"),
        );
        if is_options {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
            );
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static(
                    "Authorization, Content-Type, Accept, MCP-Protocol-Version, MCP-Method, MCP-Name, MCP-Session-Id",
                ),
            );
            headers.insert(
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("600"),
            );
        }
    }
    response
}
