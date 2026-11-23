use axum::Json;
use axum::extract::{State, rejection::JsonRejection};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde_json::{Value, json};

use super::{RESOURCE_METADATA, tools};
use crate::app::AppState;
use crate::application::mcp::oauth;
use crate::domain::mcp::{READ_WRITE, is_url_token};
use crate::infrastructure::database::queries::mcp_oauth::{self as queries, Grant};
use crate::infrastructure::security::sha256_hex;

const MODERN: &str = "2026-07-28";
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
const INSTRUCTIONS: &str = "Edite apenas o cardápio em rascunho. Leia antes de alterar registros existentes e confirme preços com o proprietário. Depois, compartilhe o link do painel para revisão e publicação.";

pub async fn stream(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match authenticate(&state, &headers).await {
        Ok(_) => (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response(),
        Err(response) => *response,
    }
}

pub async fn call(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<Value>, JsonRejection>,
) -> Response {
    let grant = match authenticate(&state, &headers).await {
        Ok(grant) => grant,
        Err(response) => return *response,
    };
    let Json(request) = match body {
        Ok(body) => body,
        Err(rejection) => {
            return rpc_error(
                rejection.status(),
                Value::Null,
                -32700,
                "Invalid JSON request",
            );
        }
    };
    let Some(method) = request
        .get("method")
        .and_then(Value::as_str)
        .filter(|_| request["jsonrpc"] == "2.0")
    else {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            Value::Null,
            -32600,
            "Invalid request",
        );
    };
    let id = request.get("id").cloned();
    if id
        .as_ref()
        .is_some_and(|id| !(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()))
    {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            Value::Null,
            -32600,
            "Invalid request id",
        );
    }
    if headers.contains_key("mcp-session-id") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let version = headers
        .get("mcp-protocol-version")
        .and_then(|value| value.to_str().ok());
    let params = &request["params"];
    let in_body = params["_meta"][META_VERSION].as_str();
    let modern = version == Some(MODERN) || in_body == Some(MODERN) || method == "server/discover";
    let id_only = id.clone().unwrap_or(Value::Null);
    if modern {
        let valid = version == Some(MODERN)
            && in_body == Some(MODERN)
            && headers.get("mcp-method").and_then(|v| v.to_str().ok()) == Some(method)
            && (method != "tools/call"
                || headers.get("mcp-name").and_then(|v| v.to_str().ok())
                    == params["name"].as_str());
        if !valid {
            return rpc_error(
                StatusCode::BAD_REQUEST,
                id_only,
                -32600,
                "MCP headers do not match request metadata",
            );
        }
    } else if version.is_some_and(|version| !LEGACY.contains(&version)) {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            id_only,
            -32600,
            "Unsupported MCP protocol version",
        );
    }
    let Some(id) = id else {
        return if method.starts_with("notifications/") {
            StatusCode::ACCEPTED.into_response()
        } else {
            rpc_error(
                StatusCode::BAD_REQUEST,
                Value::Null,
                -32600,
                "A request id is required",
            )
        };
    };
    let result = match method {
        "initialize" if !modern => {
            let version = params["protocolVersion"]
                .as_str()
                .filter(|v| LEGACY.contains(v))
                .unwrap_or(LEGACY[0]);
            json!({"protocolVersion": version, "serverInfo": identity(), "capabilities": {"tools": {}}, "instructions": INSTRUCTIONS})
        }
        "server/discover" if modern => {
            json!({"supportedVersions":[MODERN], "capabilities":{"tools":{}}, "instructions":INSTRUCTIONS})
        }
        "ping" => json!({}),
        "tools/list" => tools::catalog(),
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return rpc_error(StatusCode::OK, id, -32602, "Missing tool name");
            };
            let Some(scope) = tools::scope(name) else {
                return rpc_error(StatusCode::OK, id, -32602, "Unknown tool");
            };
            if !grant
                .scope
                .split_ascii_whitespace()
                .any(|allowed| allowed == scope)
            {
                return challenge(&state, StatusCode::FORBIDDEN, "insufficient_scope", id);
            }
            if state
                .rate_limiters
                .mcp_establishment
                .check_key(&grant.establishment_id)
                .is_err()
            {
                return rpc_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    id,
                    -32000,
                    "MCP rate limit exceeded",
                );
            }
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let mut result = match tools::execute(&state, &grant, name, args).await {
                Ok(data) => {
                    json!({"content":[{"type":"text","text":data.to_string()}],"structuredContent":data})
                }
                Err(error) => {
                    if error.status_code().is_server_error() {
                        tracing::error!(%error, tool = name, "MCP tool failed");
                    }
                    let text = state.catalog.t(crate::i18n::Locale::PtBr, error.i18n_key());
                    json!({"isError":true,"content":[{"type":"text","text":text}],"structuredContent":{"code":error.i18n_key()}})
                }
            };
            if modern {
                result["resultType"] = json!("complete");
            }
            result
        }
        _ => return rpc_error(StatusCode::OK, id, -32601, "Method not found"),
    };
    let mut result = result;
    if modern {
        result["_meta"] = json!({META_SERVER_INFO: identity()});
    }
    Json(json!({"jsonrpc":"2.0", "id":id, "result":result})).into_response()
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Grant, Box<Response>> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let allowed = origin.to_str().is_ok_and(|origin| {
            origin == state.config.public_base_url
                || matches!(
                    origin,
                    "https://chatgpt.com" | "https://claude.ai" | "https://antigravity.google"
                )
        });
        if !allowed {
            return Err(Box::new(rpc_error(
                StatusCode::FORBIDDEN,
                Value::Null,
                -32600,
                "Origin not permitted",
            )));
        }
    }
    let raw = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| header.split_once(' '))
        .filter(|(scheme, token)| {
            scheme.eq_ignore_ascii_case("Bearer") && is_url_token(token, 43, 43)
        })
        .map(|(_, token)| token);
    if let Some(raw) = raw {
        match queries::authenticate(
            &state.pool,
            &sha256_hex(raw),
            &oauth::resource(&state.config.public_base_url),
            Utc::now().timestamp(),
        )
        .await
        {
            Ok(Some(grant)) => return Ok(grant),
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, "MCP credential validation unavailable");
                return Err(Box::new(rpc_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Value::Null,
                    -32603,
                    "Authentication unavailable",
                )));
            }
        }
    }
    Err(Box::new(challenge(
        state,
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        Value::Null,
    )))
}

fn challenge(state: &AppState, status: StatusCode, code: &str, id: Value) -> Response {
    let message = if status == StatusCode::FORBIDDEN {
        "Authorize the requested menu scope"
    } else {
        "Connect your QR Cardapio account"
    };
    let value = format!(
        "Bearer resource_metadata=\"{}{RESOURCE_METADATA}\", scope=\"{READ_WRITE}\", error=\"{code}\", error_description=\"{message}\"",
        state.config.public_base_url
    );
    let Ok(header_value) = HeaderValue::from_str(&value) else {
        return rpc_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            id,
            -32603,
            "Invalid server origin",
        );
    };
    (
        status,
        [(header::WWW_AUTHENTICATE, header_value)],
        Json(
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":message,
        "data":{"_meta":{"mcp/www_authenticate":[value]}}}}),
        ),
    )
        .into_response()
}

fn identity() -> Value {
    json!({"name":"qrcardapio", "title":"QR Cardápio", "version":env!("CARGO_PKG_VERSION")})
}

fn rpc_error(status: StatusCode, id: Value, code: i32, message: &str) -> Response {
    (
        status,
        Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})),
    )
        .into_response()
}
