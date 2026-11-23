use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::mcp::{
    OAuthError, PATH, is_url_token, normalize_scope, provider_name, registered_callback,
};
use crate::infrastructure::database::queries::mcp_oauth::{
    self as queries, Authorization, CodeExchange, CredentialRotation,
};
use crate::infrastructure::security::{generate_token_32, sha256_hex};

const ACCESS_SECONDS: i64 = 3600;
const REFRESH_SECONDS: i64 = 30 * 86400;

#[derive(Deserialize)]
pub struct AuthorizeParams {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    scope: Option<String>,
    state: Option<String>,
    resource: Option<String>,
}

impl AuthorizeParams {
    pub fn validate(self, origin: &str) -> Result<Authorization, OAuthError> {
        if !registered_callback(&self.client_id, &self.redirect_uri) {
            return Err(OAuthError::InvalidClient);
        }
        let resource = validate_resource(self.resource.as_deref(), origin)?;
        if self.response_type != "code"
            || self.code_challenge_method != "S256"
            || !URL_SAFE_NO_PAD
                .decode(&self.code_challenge)
                .is_ok_and(|bytes| bytes.len() == 32)
            || self
                .state
                .as_ref()
                .is_some_and(|state| state.len() > 1024 || state.chars().any(char::is_control))
        {
            return Err(OAuthError::InvalidRequest);
        }
        let scope = normalize_scope(self.scope.as_deref()).ok_or(OAuthError::InvalidScope)?;
        Ok(Authorization {
            client_id: self.client_id,
            redirect_uri: self.redirect_uri,
            state: self.state,
            challenge: self.code_challenge,
            resource,
            scope: scope.into(),
            expires_at: Utc::now().timestamp() + 600,
        })
    }
}

#[derive(Deserialize)]
pub struct TokenForm {
    grant_type: String,
    client_id: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
    scope: Option<String>,
}

#[derive(Serialize)]
pub struct TokenResponse {
    access_token: String,
    refresh_token: String,
    token_type: &'static str,
    expires_in: i64,
    scope: String,
}

pub async fn token(
    pool: &SqlitePool,
    form: TokenForm,
    origin: &str,
) -> Result<TokenResponse, OAuthError> {
    if provider_name(&form.client_id).is_none() {
        return Err(OAuthError::InvalidClient);
    }
    let resource = validate_resource(form.resource.as_deref(), origin)?;
    let now = Utc::now().timestamp();
    let access_token = generate_token_32()?;
    let refresh_token = generate_token_32()?;
    let access_hash = sha256_hex(&access_token);
    let refresh_hash = sha256_hex(&refresh_token);
    let credentials = CredentialRotation {
        access_hash: &access_hash,
        refresh_hash: &refresh_hash,
        access_expiry: now + ACCESS_SECONDS,
    };
    let (scope, expires_at) = match form.grant_type.as_str() {
        "authorization_code" => {
            let code = form
                .code
                .filter(|code| is_url_token(code, 43, 43))
                .ok_or(OAuthError::InvalidGrant)?;
            let redirect = form.redirect_uri.ok_or(OAuthError::InvalidRequest)?;
            if !registered_callback(&form.client_id, &redirect) {
                return Err(OAuthError::InvalidGrant);
            }
            let verifier = form
                .code_verifier
                .filter(|verifier| is_url_token(verifier, 43, 128))
                .ok_or(OAuthError::InvalidGrant)?;
            let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier));
            queries::exchange_code(
                pool,
                CodeExchange {
                    hash: &sha256_hex(&code),
                    client: &form.client_id,
                    redirect: &redirect,
                    challenge: &challenge,
                    resource: &resource,
                },
                credentials,
                &Uuid::now_v7().to_string(),
                now,
                now + REFRESH_SECONDS,
            )
            .await?
            .map(|scope| (scope, now + ACCESS_SECONDS))
        }
        "refresh_token" => {
            let old = form
                .refresh_token
                .filter(|token| is_url_token(token, 43, 43))
                .ok_or(OAuthError::InvalidGrant)?;
            let requested = form
                .scope
                .as_deref()
                .map(|scope| normalize_scope(Some(scope)).ok_or(OAuthError::InvalidScope))
                .transpose()?;
            queries::rotate_refresh(
                pool,
                &sha256_hex(&old),
                &form.client_id,
                &resource,
                requested,
                credentials,
                now,
            )
            .await?
        }
        _ => return Err(OAuthError::UnsupportedGrant),
    }
    .ok_or(OAuthError::InvalidGrant)?;
    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: expires_at - now,
        scope,
    })
}

pub fn resource(origin: &str) -> String {
    format!("{origin}{PATH}")
}

fn validate_resource(requested: Option<&str>, origin: &str) -> Result<String, OAuthError> {
    let resource = resource(origin);
    if requested.is_some_and(|request| request != resource) {
        return Err(OAuthError::InvalidTarget);
    }
    Ok(resource)
}
