mod connections;
mod oauth;
mod protocol;
mod tools;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};

use crate::app::AppState;
use crate::http::middleware::security_headers::no_store;

pub const AUTHORIZE_PATH: &str = "/mcp/oauth/authorize";
pub const CONSENT_PATH: &str = "/mcp/oauth/consent";
pub const TOKEN_PATH: &str = "/mcp/oauth/token";
pub const RESOURCE_METADATA: &str = "/.well-known/oauth-protected-resource/mcp";
pub const AUTH_METADATA: &str = "/.well-known/oauth-authorization-server";

pub fn routes(enabled: bool) -> Router<AppState> {
    if !enabled {
        return Router::new();
    }
    Router::new()
        .route("/mcp", post(protocol::call).get(protocol::stream))
        .route(RESOURCE_METADATA, get(oauth::protected_resource))
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth::protected_resource),
        )
        .route(AUTH_METADATA, get(oauth::authorization_server))
        .route(AUTHORIZE_PATH, get(oauth::authorize))
        .route(
            CONSENT_PATH,
            get(oauth::consent).post(oauth::consent_submit),
        )
        .route("/mcp/oauth/login", post(oauth::login_submit))
        .route("/mcp/oauth/google/start", get(oauth::google_start))
        .route(TOKEN_PATH, post(oauth::token))
        .route("/dashboard/integrations", get(connections::page))
        .route("/dashboard/integrations/revoke", post(connections::revoke))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn(no_store))
}
