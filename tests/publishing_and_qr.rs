use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::header::{ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use image::{DynamicImage, ImageEncoder, codecs::png::PngEncoder};
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::register_owner;
use qrcardapio::application::establishments::{
    CreateEstablishmentInput, create_establishment, upload_logo,
};
use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product,
};
use qrcardapio::application::publishing::{
    PublishLocks, publish_establishment, unpublish_establishment,
};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::http::handlers::public::serve_published_file_from_disk;
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    find_establishment_for_account, find_media_asset_for_est,
};
use qrcardapio::infrastructure::security::PasswordService;
use qrcardapio::qr::{generate_png, generate_svg};

fn test_config(dir: &TempDir) -> Config {
    Config {
        asaas: None,
        mcp_enabled: false,
        google: None,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "https://qrcardapio.com.br".to_string(),
        data_dir: dir.path().join("data"),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_string(),
    }
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = DynamicImage::new_rgba8(w, h);
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out))
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .unwrap();
    out
}

#[tokio::test]
async fn atomic_publish_xss_escaping_zero_db_delivery_and_recovery() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());
    let locks = PublishLocks::default();

    let auth = register_owner(
        &pool,
        &pw,
        "pub@cantina.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();

    let est = create_establishment(
        &pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: "Cantina <script>alert('xss')</script>",
            slug: "cantina-bella",
            description: "Massas \"artesanais\" & vinhos",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();

    let logo = upload_logo(&pool, &assets, &est, png_bytes(180, 180))
        .await
        .unwrap();

    let cat = create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Massas <b>Frescas</b>",
            name_en: "Fresh Pasta",
            name_es: "Pastas Frescas",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();

    let prod = create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &cat.id,
            name: "Lasanha <img src=x onerror=alert(1)>",
            name_en: "Beef Lasagna",
            name_es: "Lasaña de Carne",
            description: "Molho da casa",
            description_en: "House ragu",
            description_es: "Salsa de la casa",
            price_raw: "42,90",
            is_available: true,
            is_visible: true,
        },
        Some(png_bytes(600, 400)),
    )
    .await
    .unwrap();

    // Unpublished menu returns 404
    let app = build_router(AppState::new(cfg.clone(), pool.clone()));
    let res_404 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/c/cantina-bella")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_404.status(), StatusCode::NOT_FOUND);
    assert!(
        String::from_utf8_lossy(&res_404.into_body().collect().await.unwrap().to_bytes())
            .contains("state-missing.svg")
    );

    // Publish v1
    let out1 = publish_establishment(&pool, &cfg, &locks, &est.id)
        .await
        .unwrap();
    assert_eq!(out1.version, 1);

    // Verify XSS escaping and embedded i18n JSON in published HTML
    let res_pub = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/c/cantina-bella")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_pub.status(), StatusCode::OK);
    let etag = res_pub
        .headers()
        .get(ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let html = String::from_utf8(
        res_pub
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        html.contains("&#60;script&#62;") || html.contains("&lt;script&gt;"),
        "expected HTML-escaped script tag"
    );
    assert!(!html.contains("<script>alert("));
    assert!(!html.contains("<img src=x onerror="));
    assert!(!html.contains("<b>Frescas</b>"));
    assert!(html.contains("id=\"qr-menu-i18n\""));
    assert!(html.contains("<details class=\"lang-picker\">"));
    assert!(html.contains("class=\"lang-current-code\">PT</span>"));
    assert!(html.contains("Beef Lasagna"));
    assert!(html.contains("Lasaña de Carne"));

    let cover_asset =
        find_media_asset_for_est(&pool, &est.id, prod.image_asset_id.as_deref().unwrap())
            .await
            .unwrap()
            .unwrap();
    let cover_url = format!("/c/cantina-bella/media/{}", cover_asset.filename_large);
    assert!(html.contains(&format!("class=\"menu-cover\" src=\"{cover_url}\"")));
    let cover_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(cover_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cover_response.status(), StatusCode::OK);

    // Conditional ETag -> 304 Not Modified
    let res_304 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/c/cantina-bella")
                .header(IF_NONE_MATCH, etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_304.status(), StatusCode::NOT_MODIFIED);

    // Path traversal blocked
    let traversal = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/c/cantina-bella/../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(traversal.status(), StatusCode::NOT_FOUND);

    // Logo static asset works from published media
    let logo_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/c/cantina-bella/media/{}", logo.filename_thumb))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logo_res.status(), StatusCode::OK);

    // Simulated failure recovery: create a 2nd product with an image, then delete its source file on disk
    // so publishing v2 fails before atomic rename; verify v1 remains live and serving!
    let prod2 = create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &cat.id,
            name: "Nhoque",
            name_en: "Gnocchi",
            name_es: "Ñoquis",
            description: "Novo",
            description_en: "New",
            description_es: "Nuevo",
            price_raw: "35,00",
            is_available: true,
            is_visible: true,
        },
        Some(png_bytes(500, 500)),
    )
    .await
    .unwrap();
    let prod2_asset_id = prod2.image_asset_id.unwrap();
    let asset2 = find_media_asset_for_est(&pool, &est.id, &prod2_asset_id)
        .await
        .unwrap()
        .unwrap();
    std::fs::remove_file(cfg.media_dir().join(&est.id).join(&asset2.filename_thumb)).unwrap();
    let pub_fail = publish_establishment(&pool, &cfg, &locks, &est.id).await;
    assert!(
        pub_fail.is_err(),
        "expected failure due to missing source image"
    );

    // Previous v1 snapshot MUST still be intact and serving!
    let still_live = serve_published_file_from_disk(
        &cfg.published_dir(),
        "cantina-bella",
        "index.html",
        Locale::PtBr,
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(still_live.status(), StatusCode::OK);
    let still_bytes = still_live.into_body().collect().await.unwrap().to_bytes();
    let still_html = String::from_utf8(still_bytes.to_vec()).unwrap();
    assert!(!still_html.contains("Nhoque"), "v1 must still be intact");

    // Remove the broken product and retry publication -> succeeds cleanly!
    qrcardapio::application::menus::delete_product(&pool, &assets, &est.id, &prod2.id)
        .await
        .unwrap();
    let out2 = publish_establishment(&pool, &cfg, &locks, &est.id)
        .await
        .unwrap();
    assert_eq!(out2.version, 2);

    // Concurrent reads during repeated publishing never observe partial HTML.
    let cfg_arc = Arc::new(cfg.clone());
    let pool_clone = pool.clone();
    let locks_clone = locks.clone();
    let est_id_clone = est.id.clone();

    let writer = tokio::spawn(async move {
        for _ in 0..5 {
            publish_establishment(&pool_clone, &cfg_arc, &locks_clone, &est_id_clone)
                .await
                .unwrap();
        }
    });

    let pub_root = cfg.published_dir();
    let reader = tokio::spawn(async move {
        for _ in 0..25 {
            let r = serve_published_file_from_disk(
                &pub_root,
                "cantina-bella",
                "index.html",
                Locale::PtBr,
                &HeaderMap::new(),
            )
            .await;
            assert_eq!(r.status(), StatusCode::OK);
            let body = r.into_body().collect().await.unwrap().to_bytes();
            let text = String::from_utf8(body.to_vec()).unwrap();
            assert!(text.starts_with("<!DOCTYPE html>"));
            assert!(text.trim_end().ends_with("</html>"));
        }
    });

    writer.await.unwrap();
    reader.await.unwrap();

    // Unpublish removes public snapshot -> returns 404.
    let est_latest = find_establishment_for_account(&pool, &auth.account.id)
        .await
        .unwrap()
        .unwrap();
    unpublish_establishment(&pool, &cfg, &locks, &est_latest)
        .await
        .unwrap();
    let after_unpub = serve_published_file_from_disk(
        &cfg.published_dir(),
        "cantina-bella",
        "index.html",
        Locale::PtBr,
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(after_unpub.status(), StatusCode::NOT_FOUND);

    // Republish once more, then close & DELETE SQLite DB to prove zero-DB public delivery!
    publish_establishment(&pool, &cfg, &locks, &est.id)
        .await
        .unwrap();
    pool.close().await;
    std::fs::remove_file(cfg.db_path()).unwrap();

    let zero_db_res = serve_published_file_from_disk(
        &cfg.published_dir(),
        "cantina-bella",
        "index.html",
        Locale::PtBr,
        &HeaderMap::new(),
    )
    .await;
    assert_eq!(zero_db_res.status(), StatusCode::OK);

    // QR Code SVG and PNG validity for permanent URL
    let qr_url = cfg.public_menu_url("cantina-bella");
    assert_eq!(qr_url, "https://qrcardapio.com.br/c/cantina-bella");
    let svg = generate_svg(&qr_url).unwrap();
    assert!(svg.contains("<svg") && svg.contains("</svg>"));
    let png = generate_png(&qr_url).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}
