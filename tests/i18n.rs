use std::collections::BTreeSet;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::header::{ACCEPT_LANGUAGE, COOKIE, ETAG, IF_NONE_MATCH};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::{Catalog, Locale};
use qrcardapio::infrastructure::database::init_pool;

fn test_config(dir: &TempDir) -> Config {
    Config {
        asaas: None,
        mcp_enabled: false,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "http://127.0.0.1:8080".to_string(),
        data_dir: dir.path().join("data"),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 2,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_string(),
    }
}

#[tokio::test]
async fn i18n_catalog_parity_and_http_zero_reload_contracts() {
    // 1. Key parity across pt-BR, en, and es
    let cat = Catalog::global();
    let pt: BTreeSet<&String> = cat.map_for(Locale::PtBr).keys().collect();
    let en: BTreeSet<&String> = cat.map_for(Locale::En).keys().collect();
    let es: BTreeSet<&String> = cat.map_for(Locale::Es).keys().collect();
    assert_eq!(pt, en);
    assert_eq!(pt, es);

    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg, pool.clone()));

    // 2. Serve embedded JSON dictionaries with ETag and 304 support
    for code in ["pt-BR", "en", "es"] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/static/i18n/{code}.json?v={}", cat.version_hash()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let etag = res
            .headers()
            .get(ETAG)
            .expect("ETag header")
            .to_str()
            .unwrap()
            .to_string();

        let res304 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/static/i18n/{code}.json"))
                    .header(IF_NONE_MATCH, etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res304.status(), StatusCode::NOT_MODIFIED);
    }

    // 3. Server-side rendering respects ?lang=, qr_lang cookie, and Accept-Language,
    //    and attaches data-i18n hooks + CSP headers with zero inline JS/styles.
    let res_es = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/?lang=es")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_es.status(), StatusCode::OK);
    let csp = res_es
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("style-src 'self'"));

    let html_es = String::from_utf8(
        res_es
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(html_es.contains("<html lang=\"es\""));
    assert!(html_es.contains("Tu menú."));
    assert!(html_es.contains("Siempre a mano."));
    assert!(html_es.contains("Brasa 128"));
    assert!(html_es.contains("Resumen de reseñas"));
    assert!(html_es.contains("Perfil ficticio · Datos simulados"));
    assert!(html_es.contains("href=\"#maps-demo\""));
    assert!(html_es.contains("id=\"maps-demo\""));
    assert!(html_es.contains("data-i18n-alt=\"maps.storefront_alt\""));
    assert!(html_es.contains("data-i18n=\"landing.hero_title\""));
    assert!(html_es.contains("/static/js/i18n.js"));
    assert!(html_es.contains("<details class=\"lang-picker\">"));
    assert!(html_es.contains("class=\"lang-current-code\">ES</span>"));
    assert!(html_es.contains("data-lang-btn=\"es\" aria-current=\"true\""));
    assert!(
        !html_es.contains("style=\""),
        "strict CSP forbids inline styles"
    );

    let res_cookie = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/login")
                .header(COOKIE, "qr_lang=en")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html_en = String::from_utf8(
        res_cookie
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(html_en.contains("<html lang=\"en\""));
    assert!(html_en.contains("Sign in to your dashboard"));

    let res_accept = app
        .oneshot(
            Request::builder()
                .uri("/register")
                .header(ACCEPT_LANGUAGE, "es-419,es;q=0.9,pt-BR;q=0.7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html_accept = String::from_utf8(
        res_accept
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(html_accept.contains("<html lang=\"es\""));
    assert!(html_accept.contains("Crea tu cuenta"));

    pool.close().await;
}
