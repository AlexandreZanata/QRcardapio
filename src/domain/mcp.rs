use crate::error::AppError;

pub const PATH: &str = "/mcp";
pub const READ: &str = "menu.read";
pub const WRITE: &str = "menu.write";
pub const READ_WRITE: &str = "menu.read menu.write";

const CHATGPT: &str = "https://chatgpt.com/oauth/client.json";
const CLAUDE: &str = "https://claude.ai/oauth/mcp-oauth-client-metadata";
const ANTIGRAVITY: &str = "https://antigravity.google/oauth/client-metadata.json";

pub fn provider_name(client: &str) -> Option<&'static str> {
    match client {
        CHATGPT => Some("ChatGPT"),
        CLAUDE => Some("Claude"),
        ANTIGRAVITY => Some("Google Antigravity"),
        _ => None,
    }
}

pub fn registered_callback(client: &str, redirect: &str) -> bool {
    matches!(
        (client, redirect),
        (
            CHATGPT,
            "https://chatgpt.com/connector_platform_oauth_redirect"
        ) | (CLAUDE, "https://claude.ai/api/mcp/auth_callback")
            | (ANTIGRAVITY, "https://antigravity.google/oauth-callback")
    )
}

pub fn normalize_scope(scope: Option<&str>) -> Option<&'static str> {
    let mut read = false;
    let mut write = false;
    for item in scope.unwrap_or(READ_WRITE).split_ascii_whitespace() {
        match item {
            READ => read = true,
            WRITE => write = true,
            _ => return None,
        }
    }
    match (read, write) {
        (true, false) => Some(READ),
        (_, true) => Some(READ_WRITE),
        _ => None,
    }
}

pub fn is_url_token(raw: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("invalid_request")]
    InvalidRequest,
    #[error("invalid_client")]
    InvalidClient,
    #[error("invalid_scope")]
    InvalidScope,
    #[error("invalid_target")]
    InvalidTarget,
    #[error("invalid_grant")]
    InvalidGrant,
    #[error("unsupported_grant_type")]
    UnsupportedGrant,
    #[error("server_error")]
    Internal(#[from] AppError),
}
