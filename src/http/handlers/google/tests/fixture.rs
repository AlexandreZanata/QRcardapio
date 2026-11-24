use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use aws_lc_rs::{
    rand::SystemRandom,
    rsa::{KeyPair, KeySize},
    signature::{KeyPair as _, RSA_PKCS1_SHA256},
};
use axum::{
    Router,
    body::Body,
    http::{Request, header},
    response::Response,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use http_body_util::BodyExt;
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;

use crate::{
    app::{AppState, build_router},
    config::{Config, DbSynchronous, GoogleConfig, LogFormat},
    infrastructure::{database::init_pool, google::GoogleResponse},
};

pub const ORIGIN: &str = "https://menu.example.test";
const CLIENT_ID: &str = "test-qr-cardapio.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "fixture-value";
pub const EMAIL: &str = "owner@example.test";
pub const SUBJECT: &str = "google-fixture-subject";
pub const PASSWORD: &str = "StrongFixturePassword123";
pub const COOKIE_NAME: &str = "qr_google_flow";
pub const SESSION_COOKIE: &str = "qr_session";
pub const FLASH_COOKIE: &str = "qr_flash";

pub fn contract() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contracts/auth/google.json"
    )))
    .unwrap()
}

struct ExpectedExchange {
    challenge: String,
    claims: Value,
    bad_signature: bool,
}

pub struct Provider {
    pending: Mutex<HashMap<String, ExpectedExchange>>,
    pub exchanges: AtomicUsize,
    pub key_requests: AtomicUsize,
    key: KeyPair,
}

impl Provider {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::default(),
            exchanges: AtomicUsize::default(),
            key_requests: AtomicUsize::default(),
            key: KeyPair::generate(KeySize::Rsa2048).unwrap(),
        })
    }

    pub fn authorize(&self, flow: &Flow, code: &str, claims: Value, bad_signature: bool) {
        let email = claims
            .get("email_verified")
            .cloned()
            .unwrap_or(Value::Bool(true));
        let now = Utc::now().timestamp();
        let mut all = json!({
            "iss": contract()["issuers"][0], "aud": CLIENT_ID, "azp": CLIENT_ID, "sub": SUBJECT,
            "email": EMAIL, "email_verified": email, "nonce": flow.params["nonce"],
            "exp": now + 3600, "iat": now,
        });
        all.as_object_mut()
            .unwrap()
            .extend(claims.as_object().unwrap().clone());
        assert!(
            self.pending
                .lock()
                .unwrap()
                .insert(
                    code.into(),
                    ExpectedExchange {
                        challenge: flow.params["code_challenge"].clone(),
                        claims: all,
                        bad_signature,
                    }
                )
                .is_none()
        );
    }

    fn reply(&self, request: reqwest::Request) -> GoogleResponse {
        let contract = contract();
        let mut headers = header::HeaderMap::new();
        let value = if request.url().as_str() == contract["token_endpoint"] {
            assert_eq!(*request.method(), reqwest::Method::POST);
            assert_eq!(
                request.headers()[header::CONTENT_TYPE],
                "application/x-www-form-urlencoded"
            );
            let body = std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap();
            let params = query(&Url::parse(&format!("{ORIGIN}/?{body}")).unwrap());
            assert_eq!(params.len(), 6);
            assert_eq!(params["grant_type"], "authorization_code");
            assert_eq!(params["client_id"], CLIENT_ID);
            assert_eq!(params["client_secret"], CLIENT_SECRET);
            assert_eq!(
                params["redirect_uri"],
                format!("{ORIGIN}{}", contract["redirect_path"].as_str().unwrap())
            );
            self.exchanges.fetch_add(1, Ordering::Relaxed);
            let grant = self
                .pending
                .lock()
                .unwrap()
                .remove(&params["code"])
                .expect("registered Google fixture code");
            assert_eq!(
                URL_SAFE_NO_PAD.encode(Sha256::digest(&params["code_verifier"])),
                grant.challenge
            );
            let head = URL_SAFE_NO_PAD
                .encode(json!({"alg":contract["algorithm"],"kid":"fixture-key"}).to_string());
            let claims = URL_SAFE_NO_PAD.encode(grant.claims.to_string());
            let message = format!("{head}.{claims}");
            let mut signature = vec![0; 256];
            self.key
                .sign(
                    &RSA_PKCS1_SHA256,
                    &SystemRandom::new(),
                    message.as_bytes(),
                    &mut signature,
                )
                .unwrap();
            if grant.bad_signature {
                signature[0] ^= 1;
            }
            json!({"id_token":format!("{message}.{}",URL_SAFE_NO_PAD.encode(signature)),
                "access_token":"provider-access-fixture","token_type":"Bearer","expires_in":3600})
        } else {
            assert_eq!(request.url().as_str(), contract["jwks_uri"]);
            assert_eq!(*request.method(), reqwest::Method::GET);
            self.key_requests.fetch_add(1, Ordering::Relaxed);
            // AWS-LC returns a PKCS#1 DER sequence of two positive integers.
            let mut der = self.key.public_key().as_ref();
            let mut public = der_value(&mut der, 0x30);
            assert!(der.is_empty());
            let modulus = der_value(&mut public, 0x02);
            let exponent = der_value(&mut public, 0x02);
            assert!(public.is_empty());
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=300".parse().unwrap(),
            );
            json!({"keys":[{"kty":"RSA","alg":"RS256","use":"sig","kid":"fixture-key",
                "n":URL_SAFE_NO_PAD.encode(modulus.strip_prefix(&[0]).unwrap_or(modulus)),"e":URL_SAFE_NO_PAD.encode(exponent)}]})
        };
        GoogleResponse {
            headers,
            body: value.to_string().into_bytes(),
        }
    }
}

pub fn google_config(origin: &str, secure: bool) -> Result<GoogleConfig, String> {
    GoogleConfig::new(CLIENT_ID, CLIENT_SECRET, origin, secure)
}

pub async fn setup(enabled: bool, billing: bool) -> (TempDir, SqlitePool, Router, Arc<Provider>) {
    let dir = TempDir::new().unwrap();
    let config = Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: ORIGIN.into(),
        data_dir: dir.path().into(),
        static_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: true,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".into(),
        asaas: billing.then(|| {
            crate::config::AsaasConfig::new(
                crate::config::AsaasEnvironment::Sandbox,
                "fixture-key",
                "fixture-webhook-with-at-least-32-characters",
            )
            .unwrap()
        }),
        mcp_enabled: true,
        google: enabled.then(|| google_config(ORIGIN, true).unwrap()),
    };
    let pool = init_pool(&config).await.unwrap();
    let mut state = AppState::new(config, pool.clone());
    let provider = Provider::new();
    if let Some(google) = &mut state.google {
        let mock = provider.clone();
        google.set_fixture(Arc::new(move |request| Ok(mock.reply(request))));
    }
    (dir, pool, build_router(state), provider)
}

pub async fn get(app: &Router, path: &str, cookies: &str) -> Response {
    app.clone()
        .oneshot(
            Request::get(path)
                .header(header::COOKIE, cookies)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

pub async fn post(app: &Router, path: &str, cookies: &str, form: &[(&str, &str)]) -> Response {
    let encoded = Url::parse_with_params(ORIGIN, form).unwrap();
    app.clone()
        .oneshot(
            Request::post(path)
                .header(header::COOKIE, cookies)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(encoded.query().unwrap().to_owned()))
                .unwrap(),
        )
        .await
        .unwrap()
}

pub fn cookie(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|v| v.starts_with(&format!("{name}=")))
        .map(str::to_owned)
}

pub fn browser_cookie(response: &Response, name: &str) -> String {
    cookie(response, name)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

pub fn location(response: &Response) -> &str {
    response.headers()[header::LOCATION].to_str().unwrap()
}

pub async fn html(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

pub fn field<'a>(page: &'a str, prefix: &str) -> &'a str {
    page.split_once(prefix)
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

pub fn csrf(page: &str) -> &str {
    field(page, "name=\"_csrf\" value=\"")
}

pub fn query(url: &Url) -> HashMap<String, String> {
    url.query_pairs().into_owned().collect()
}

pub struct Flow {
    pub browser: String,
    pub params: HashMap<String, String>,
}

impl Flow {
    pub fn new(url: &str, browser: String) -> Self {
        let c = contract();
        let url = Url::parse(url).unwrap();
        let mut endpoint = url.clone();
        endpoint.set_query(None);
        assert_eq!(endpoint.as_str(), c["authorization_endpoint"]);
        let params = query(&url);
        assert_eq!(params["scope"], c["scope"]);
        assert_eq!(params["client_id"], CLIENT_ID);
        assert_eq!(
            params["redirect_uri"],
            format!("{ORIGIN}{}", c["redirect_path"].as_str().unwrap())
        );
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["access_type"], "online");
        for key in ["state", "nonce", "code_challenge"] {
            assert_eq!(params[key].len(), 43);
        }
        assert_ne!(params["nonce"], params["state"]);
        assert!(!params.contains_key("client_secret"));
        Self { browser, params }
    }

    pub async fn start(app: &Router, path: &str, cookies: &str) -> Self {
        let response = get(app, path, cookies).await;
        assert_eq!(response.status(), 303);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        let raw = cookie(&response, COOKIE_NAME).unwrap();
        for flag in ["Path=/", "HttpOnly", "SameSite=Lax", "Secure"] {
            assert!(raw.contains(flag));
        }
        Self::new(location(&response), browser_cookie(&response, COOKIE_NAME))
    }

    pub async fn callback(&self, app: &Router, key: &str, value: &str, local: &str) -> Response {
        let path = contract()["redirect_path"].as_str().unwrap().to_owned();
        let url = Url::parse_with_params(
            &format!("{ORIGIN}{path}"),
            [("state", self.params["state"].as_str()), (key, value)],
        )
        .unwrap();
        get(
            app,
            &format!("{path}?{}", url.query().unwrap()),
            &format!("{}; {local}", self.browser),
        )
        .await
    }
}

fn der_value<'a>(bytes: &mut &'a [u8], tag: u8) -> &'a [u8] {
    let (head, rest) = bytes.split_at(2);
    assert_eq!(head[0], tag);
    let (len, content) = if head[1] < 128 {
        (usize::from(head[1]), rest)
    } else {
        let (length, content) = rest.split_at(usize::from(head[1] & 0x7f));
        let len = length
            .iter()
            .fold(0, |len, byte| (len << 8) + usize::from(*byte));
        (len, content)
    };
    let (value, rest) = content.split_at(len);
    *bytes = rest;
    value
}
