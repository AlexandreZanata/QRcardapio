use std::path::PathBuf;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::{
    app::{AppState, build_router},
    application::{
        auth::{AuthSuccess, register_owner},
        establishments::{CreateEstablishmentInput, create_establishment},
    },
    config::{Config, DbSynchronous, LogFormat},
    i18n::Locale,
    infrastructure::{database::init_pool, security::PasswordService},
};

pub(super) const ORIGIN: &str = "https://menu.example.test";
pub(super) const RESOURCE: &str = "https://menu.example.test/mcp";
pub(super) const CHATGPT: (&str, &str) = (
    "https://chatgpt.com/oauth/client.json",
    "https://chatgpt.com/connector_platform_oauth_redirect",
);
pub(super) const CLAUDE: (&str, &str) = (
    "https://claude.ai/oauth/mcp-oauth-client-metadata",
    "https://claude.ai/api/mcp/auth_callback",
);
pub(super) const GOOGLE: (&str, &str) = (
    "https://antigravity.google/oauth/client-metadata.json",
    "https://antigravity.google/oauth-callback",
);
pub(super) const VERIFIER: &str = "QrcardapioFixtureVerifierWithEnoughCharactersForPkce";
const PASSWORD: &str = "LocalFixturePasswordForTest123";

pub(super) async fn setup(enabled: bool) -> (TempDir, SqlitePool, Router) {
    setup_with_billing(enabled, false).await
}

pub(super) async fn setup_with_billing(
    enabled: bool,
    with_billing: bool,
) -> (TempDir, SqlitePool, Router) {
    let dir = TempDir::new().unwrap();
    let asaas = with_billing.then(|| {
        qrcardapio::config::AsaasConfig::new(
            qrcardapio::config::AsaasEnvironment::Sandbox,
            "test-api-key",
            "test-webhook-token-at-least-32-chars-long",
        )
        .unwrap()
    });
    let config = Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: ORIGIN.into(),
        data_dir: dir.path().into(),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: true,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".into(),
        asaas,
        mcp_enabled: enabled,
        google: None,
    };
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));
    (dir, pool, app)
}

pub(super) async fn owner(pool: &SqlitePool, slug: &str) -> (AuthSuccess, String) {
    let auth = register_owner(
        pool,
        &PasswordService::new(),
        &format!("{slug}@example.test"),
        PASSWORD,
        PASSWORD,
        Locale::PtBr,
    )
    .await
    .unwrap();
    let est = create_establishment(
        pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: "Cafeteria",
            slug,
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();
    (auth, est.id)
}

pub(super) async fn request(
    app: &Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: String,
) -> Response {
    let mut req = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    app.clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

pub(super) async fn form(
    app: &Router,
    path: &str,
    values: &[(&str, &str)],
    cookie: &str,
) -> Response {
    let body = reqwest::Url::parse_with_params(ORIGIN, values).unwrap();
    request(
        app,
        "POST",
        path,
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("cookie", cookie),
        ],
        body.query().unwrap().to_owned(),
    )
    .await
}

pub(super) async fn body(response: Response) -> String {
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
pub(super) async fn json_body(response: Response) -> Value {
    serde_json::from_str(&body(response).await).unwrap()
}

pub(super) fn cookie(response: &Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find(|cookie| cookie.starts_with(&format!("{name}=")))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

pub(super) fn field<'a>(html: &'a str, marker: &str) -> &'a str {
    html.split_once(marker)
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

pub(super) async fn authorize(
    app: &Router,
    client: (&str, &str),
    account: Option<&AuthSuccess>,
    scope: &str,
) -> String {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER));
    let authorize = Url::parse_with_params(
        &format!("{ORIGIN}/mcp/oauth/authorize"),
        [
            ("client_id", client.0),
            ("redirect_uri", client.1),
            ("response_type", "code"),
            ("state", "opaque-state"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("resource", RESOURCE),
            ("scope", scope),
        ],
    )
    .unwrap();
    let path = format!("{}?{}", authorize.path(), authorize.query().unwrap());
    let response = request(app, "GET", &path, &[], String::new()).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let browser = cookie(&response, "qr_mcp_browser");
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    let flow_id = location.split_once("?flow=").unwrap().1;
    let wrong_browser = request(app, "GET", location, &[], String::new()).await;
    assert_eq!(wrong_browser.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let session = if let Some(account) = account {
        format!("qr_session={}", account.raw_session_token)
    } else {
        let login_page =
            request(app, "GET", location, &[("cookie", &browser)], String::new()).await;
        let login_html = body(login_page).await;
        let login = form(
            app,
            "/mcp/oauth/login",
            &[
                ("flow", flow_id),
                ("_csrf", field(&login_html, "name=\"_csrf\" value=\"")),
                ("email", "test-cafe@example.test"),
                ("password", PASSWORD),
            ],
            &browser,
        )
        .await;
        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        cookie(&login, "qr_session")
    };
    let cookies = format!("{browser}; {session}");
    let page = request(app, "GET", location, &[("cookie", &cookies)], String::new()).await;
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(page.headers()[header::REFERRER_POLICY], "no-referrer");
    let consent = body(page).await;
    let uri = Url::parse(client.1).unwrap();
    assert!(consent.contains(&format!("<strong>{}</strong>", uri.host_str().unwrap())));
    let fields = [
        ("flow", flow_id),
        ("_csrf", field(&consent, "name=\"_csrf\" value=\"")),
        ("decision", "allow"),
    ];
    let complete = form(app, "/mcp/oauth/consent", &fields, &cookies).await;
    assert_eq!(complete.status(), StatusCode::OK);
    let html = body(complete).await;
    let query = field(&html, &format!("href=\"{}?", client.1)).replace("&#38;", "&");
    let approved = Url::parse(&format!("{}?{query}", client.1)).unwrap();
    let params: std::collections::HashMap<_, _> = approved.query_pairs().into_owned().collect();
    assert_eq!(params["state"], "opaque-state");
    assert_eq!(params["iss"], ORIGIN);
    assert_eq!(
        form(app, "/mcp/oauth/consent", &fields, &cookies)
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    params["code"].clone()
}

pub(super) async fn code_exchange(
    app: &Router,
    client: (&str, &str),
    code: &str,
    verifier: &str,
) -> Response {
    form(
        app,
        "/mcp/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client.0),
            ("redirect_uri", client.1),
            ("code", code),
            ("code_verifier", verifier),
            ("resource", RESOURCE),
        ],
        "",
    )
    .await
}

pub(super) async fn rpc(app: &Router, access: &str, method: &str, params: Value) -> Response {
    request(
        app,
        "POST",
        "/mcp",
        &[
            ("content-type", "application/json"),
            ("authorization", &format!("Bearer {access}")),
        ],
        json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
    )
    .await
}

pub(super) async fn tool(app: &Router, access: &str, name: &str, args: Value) -> Value {
    let res = rpc(
        app,
        access,
        "tools/call",
        json!({"name":name,"arguments":args}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    json_body(res).await["result"].clone()
}
