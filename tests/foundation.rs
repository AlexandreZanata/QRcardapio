use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::infrastructure::database::{init_pool, query_sqlite_version};

fn test_config(dir: &TempDir) -> Config {
    let static_dir = dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();
    Config {
        asaas: None,
        mcp_enabled: false,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "http://127.0.0.1:8080".to_string(),
        data_dir: dir.path().join("data"),
        static_dir,
        db_max_connections: 2,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_string(),
    }
}

#[tokio::test]
async fn sqlite_version_and_health_endpoints() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();

    let version = query_sqlite_version(&pool).await.unwrap();
    assert!(
        version.starts_with("3.54."),
        "expected bundled SQLite 3.54.x, got {version}"
    );

    let app = build_router(AppState::new(cfg, pool.clone()));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let ready = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let bytes = ready.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body_str.contains("3.54.1"), "readyz body: {body_str}");

    let i18n_res = app
        .oneshot(
            Request::builder()
                .uri("/static/i18n/en.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(i18n_res.status(), StatusCode::OK);
    let i18n_bytes = i18n_res.into_body().collect().await.unwrap().to_bytes();
    let i18n_str = String::from_utf8(i18n_bytes.to_vec()).unwrap();
    assert!(i18n_str.contains("Create a beautiful digital menu"));

    pool.close().await;
}
