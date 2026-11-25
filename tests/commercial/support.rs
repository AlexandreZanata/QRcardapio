//! Shared fixtures for the commercial, entitlement and migration suites.
#![allow(dead_code)]

use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::{AuthSuccess, register_owner};
use qrcardapio::application::establishments::{CreateEstablishmentInput, create_establishment};
use qrcardapio::config::{AsaasConfig, AsaasEnvironment, Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::security::PasswordService;

pub const ENV: &str = "sandbox";

pub fn token() -> String {
    "w".repeat(40)
}

pub fn config(dir: &TempDir, billing: bool) -> Config {
    Config {
        mcp_enabled: false,
        google: None,
        asaas: billing.then(|| {
            AsaasConfig::new(AsaasEnvironment::Sandbox, "test-api-credential", &token()).unwrap()
        }),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "https://menu.example.test".to_owned(),
        data_dir: dir.path().to_owned(),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: true,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_owned(),
    }
}

pub struct Harness {
    pub dir: TempDir,
    pub pool: SqlitePool,
    pub app: Router,
}

pub async fn harness(billing: bool) -> Harness {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir, billing);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg, pool.clone()));
    Harness { dir, pool, app }
}

pub async fn owner(pool: &SqlitePool, slug: &str) -> (AuthSuccess, String) {
    let login = register_owner(
        pool,
        &PasswordService::new(),
        &format!("{slug}@example.test"),
        "TestPassword123",
        "TestPassword123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    let est = create_establishment(
        pool,
        &login.account.id,
        CreateEstablishmentInput {
            name: "Commercial Test",
            slug,
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();
    (login, est.id)
}

/// Registers a second account and attaches it as a manager of `est`.
pub async fn manager(pool: &SqlitePool, est: &str, email: &str) -> AuthSuccess {
    let login = register_owner(
        pool,
        &PasswordService::new(),
        email,
        "TestPassword123",
        "TestPassword123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO establishment_members (establishment_id, account_id, member_role, created_at)
         VALUES (?, ?, 'manager', datetime('now'))",
    )
    .bind(est)
    .bind(&login.account.id)
    .execute(pool)
    .await
    .unwrap();
    login
}

pub async fn admin(pool: &SqlitePool, email: &str) {
    qrcardapio::application::admin::bootstrap_admin(
        pool,
        &PasswordService::new(),
        email,
        "AdminPassword123",
    )
    .await
    .unwrap();
}

pub async fn webhook(app: &Router, event: Value) -> StatusCode {
    let request = Request::post("/webhooks/asaas")
        .header(header::CONTENT_TYPE, "application/json")
        .header("asaas-access-token", token());
    app.clone()
        .oneshot(request.body(Body::from(event.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

pub async fn post_form(
    app: &Router,
    login: &AuthSuccess,
    path: &str,
    form: String,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::post(path)
                .header(
                    header::COOKIE,
                    format!("qr_session={}", login.raw_session_token),
                )
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap()
}

pub async fn get(app: &Router, login: &AuthSuccess, path: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::get(path)
                .header(
                    header::COOKIE,
                    format!("qr_session={}", login.raw_session_token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

pub fn location(response: &axum::response::Response) -> String {
    response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned()
}

/// Links an open checkout to a subscription through webhooks and returns nothing.
pub async fn link_subscription(
    app: &Router,
    checkout: &str,
    subscription: &str,
    day: &str,
    key: &str,
) {
    let link = json!({"id": format!("{key}-link"), "event": "SUBSCRIPTION_CREATED",
        "dateCreated": format!("{day} 09:00:00"),
        "subscription": {"id": subscription, "checkoutSession": checkout, "status": "ACTIVE"}});
    assert_eq!(webhook(app, link).await, StatusCode::OK);
}

pub fn payment_event(
    event_id: &str,
    kind: &str,
    payment: &str,
    subscription: &str,
    due: &str,
    at: &str,
    value: Value,
) -> Value {
    json!({
        "id": event_id, "event": kind, "dateCreated": at,
        "payment": {"id": payment, "subscription": subscription, "dueDate": due, "value": value}
    })
}
