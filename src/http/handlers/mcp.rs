mod connections;
mod oauth;
mod protocol;
mod tools;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderValue, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};

use crate::app::AppState;

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
        .route(TOKEN_PATH, post(oauth::token))
        .route("/dashboard/integrations", get(connections::page))
        .route("/dashboard/integrations/revoke", post(connections::revoke))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn(no_store))
}

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}
