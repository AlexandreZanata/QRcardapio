use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, COOKIE, LOCATION};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::{AuthSuccess, register_owner};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::find_establishment_for_account;

async fn submit(app: &Router, account: &AuthSuccess, path: &str, fields: &str) -> Response {
    app.clone()
        .oneshot(
            Request::post(path)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(COOKIE, format!("qr_session={}", account.raw_session_token))
                .body(Body::from(format!("_csrf={}&{fields}", account.csrf_token)))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn concurrent_onboarding_gives_identical_names_distinct_short_public_links() {
    let dir = TempDir::new().unwrap();
    let config = Config {
        asaas: None,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "https://qrcardapio.com.br".into(),
        data_dir: dir.path().join("data"),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".into(),
    };
    let pool = init_pool(&config).await.unwrap();
    let state = AppState::new(config, pool.clone());
    let password = "SamplePasswordForTest123";
    let first = register_owner(
        &pool,
        &state.passwords,
        "owner1@example.test",
        password,
        password,
        Locale::PtBr,
    )
    .await
    .unwrap();
    let second = register_owner(
        &pool,
        &state.passwords,
        "owner2@example.test",
        password,
        password,
        Locale::PtBr,
    )
    .await
    .unwrap();
    let app = build_router(state);

    let first_form = "name=Bar&description=Unidade+Norte";
    let (a, duplicate_submit, b) = tokio::join!(
        submit(&app, &first, "/onboarding", first_form),
        submit(&app, &first, "/onboarding", first_form),
        submit(
            &app,
            &second,
            "/onboarding",
            "name=Bar&slug=&description=Unidade+Sul"
        ),
    );
    for response in [a, duplicate_submit, b] {
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[LOCATION], "/dashboard/menu");
    }

    let north = find_establishment_for_account(&pool, &first.account.id)
        .await
        .unwrap()
        .unwrap();
    let south = find_establishment_for_account(&pool, &second.account.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(north.name, south.name);
    assert_ne!(north.id, south.id);
    let mut slugs = [&north.slug, &south.slug];
    slugs.sort();
    assert_eq!(slugs[0], "bar");
    assert_eq!(slugs[1].strip_prefix("bar-").unwrap().len(), 4);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM establishments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);

    for (account, slug, expected, unexpected) in [
        (&first, &north.slug, "Unidade Norte", "Unidade Sul"),
        (&second, &south.slug, "Unidade Sul", "Unidade Norte"),
    ] {
        assert_eq!(
            submit(&app, account, "/dashboard/publish", "")
                .await
                .status(),
            StatusCode::SEE_OTHER
        );
        let menu = app
            .clone()
            .oneshot(
                Request::get(format!("/c/{slug}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(menu.status(), StatusCode::OK);
        let html = menu.into_body().collect().await.unwrap().to_bytes();
        let html = std::str::from_utf8(&html).unwrap();
        assert!(html.contains(expected));
        assert!(!html.contains(unexpected));
    }
}
