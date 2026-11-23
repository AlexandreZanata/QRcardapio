mod claims;

pub use claims::GoogleIdentity;

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use reqwest::{Client, RequestBuilder, Url, header};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::config::GoogleConfig;
use crate::error::{AppError, AppResult};
use claims::{KeyDocument, SignedToken, SigningKey};

pub const AUTHORIZATION_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const BODY_LIMIT: usize = 64 * 1024;

#[derive(Clone)]
pub struct GoogleOidc {
    http: Client,
    config: GoogleConfig,
    cache: Arc<Mutex<Option<KeyCache>>>,
    #[cfg(test)]
    fixture: Option<FixtureClient>,
}

struct KeyCache {
    keys: HashMap<String, SigningKey>,
    loaded_at: Instant,
    expires_at: Instant,
}

pub struct GoogleResponse {
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

#[cfg(test)]
pub(crate) type FixtureClient =
    Arc<dyn Fn(reqwest::Request) -> AppResult<GoogleResponse> + Send + Sync>;

impl GoogleOidc {
    pub fn new(config: GoogleConfig) -> Self {
        let http = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("QRCardapio/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Google HTTP client configuration is valid");
        Self {
            http,
            config,
            cache: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            fixture: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_fixture(&mut self, fixture: FixtureClient) {
        self.fixture = Some(fixture);
    }

    pub fn config(&self) -> &GoogleConfig {
        &self.config
    }

    pub fn authorization_url(
        &self,
        state: &str,
        nonce: &str,
        verifier: &str,
        linking: bool,
    ) -> Url {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier));
        let mut url = Url::parse(AUTHORIZATION_URL).expect("valid Google URL");
        url.query_pairs_mut().extend_pairs([
            ("client_id", self.config.client_id.as_str()),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "openid email"),
            ("state", state),
            ("nonce", nonce),
            ("access_type", "online"),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge),
        ]);
        if linking {
            url.query_pairs_mut()
                .append_pair("prompt", "select_account");
        }
        url
    }

    pub async fn exchange(
        &self,
        code: &str,
        verifier: &str,
        nonce_hash: &str,
    ) -> AppResult<GoogleIdentity> {
        #[derive(Deserialize)]
        struct Tokens {
            id_token: String,
        }
        let request = self.http.post(TOKEN_URL).form(&[
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.secret()),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ]);
        let response = self.send(request).await?;
        let tokens: Tokens =
            serde_json::from_slice(&response.body).map_err(|_| claims::invalid())?;
        let jwt = SignedToken::parse(&tokens.id_token)?;
        let key = self.key(&jwt.kid).await?;
        jwt.verify(
            &key,
            &self.config.client_id,
            nonce_hash,
            Utc::now().timestamp(),
        )
    }

    async fn key(&self, kid: &str) -> AppResult<SigningKey> {
        let mut cache = self.cache.lock().await;
        if let Some(entry) = cache
            .as_ref()
            .filter(|entry| entry.expires_at > Instant::now())
        {
            if let Some(key) = entry.keys.get(kid) {
                return Ok(key.clone());
            }
            if entry.loaded_at.elapsed() < Duration::from_secs(30) {
                return Err(claims::invalid());
            }
        }
        let response = self.send(self.http.get(JWKS_URL)).await?;
        let document: KeyDocument =
            serde_json::from_slice(&response.body).map_err(|_| unavailable())?;
        let keys: HashMap<_, _> = document
            .keys
            .into_iter()
            .filter(SigningKey::signing_key)
            .map(|key| (key.kid.clone(), key))
            .collect();
        let key = keys.get(kid).cloned();
        let loaded_at = Instant::now();
        *cache = Some(KeyCache {
            keys,
            loaded_at,
            expires_at: loaded_at + cache_lifetime(&response.headers),
        });
        key.ok_or_else(claims::invalid)
    }

    async fn send(&self, request: RequestBuilder) -> AppResult<GoogleResponse> {
        let request = request.build().map_err(|_| unavailable())?;
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture(request);
        }
        let mut response = self
            .http
            .execute(request)
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            if response.url().as_str() == TOKEN_URL
                && response.status() == reqwest::StatusCode::BAD_REQUEST
            {
                return Err(claims::invalid());
            }
            tracing::warn!(status = %response.status(), "Google OIDC request failed");
            return Err(unavailable());
        }
        if response
            .content_length()
            .is_some_and(|len| len > BODY_LIMIT as u64)
        {
            return Err(unavailable());
        }
        let headers = response.headers().clone();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if body.len() + chunk.len() > BODY_LIMIT {
                return Err(unavailable());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(GoogleResponse { headers, body })
    }
}

fn cache_lifetime(headers: &reqwest::header::HeaderMap) -> Duration {
    let raw = headers
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if raw
        .split(',')
        .map(str::trim)
        .any(|part| matches!(part, "no-cache" | "no-store"))
    {
        return Duration::ZERO;
    }
    let max = raw
        .split(',')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("max-age="))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let age = headers
        .get(header::AGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    Duration::from_secs(max.saturating_sub(age).min(86400))
}

fn unavailable() -> AppError {
    AppError::Unavailable("google.unavailable")
}
