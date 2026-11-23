use std::io::Cursor;
use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, ETAG, IF_NONE_MATCH, LOCATION};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use image::{DynamicImage, ImageEncoder, codecs::png::PngEncoder};
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::register_owner;
use qrcardapio::application::establishments::{CreateEstablishmentInput, create_establishment};
use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product,
};
use qrcardapio::application::publishing::{
    PublishLocks, publish_establishment, unpublish_establishment,
};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::find_establishment_for_account;
use qrcardapio::infrastructure::security::PasswordService;
use qrcardapio::seo::guides::GuideId;
use qrcardapio::seo::segments::SegmentId;
use qrcardapio::seo::tools::ToolId;

fn test_config(dir: &TempDir) -> Config {
    Config {
        asaas: None,
        mcp_enabled: false,
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

async fn body_bytes(res: axum::response::Response) -> Vec<u8> {
    res.into_body().collect().await.unwrap().to_bytes().to_vec()
}

async fn body_str(res: axum::response::Response) -> String {
    String::from_utf8(body_bytes(res).await).unwrap()
}

fn extract_json_lds(html: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let start_marker = "<script type=\"application/ld+json\">";
    let end_marker = "</script>";
    let mut rest = html;
    while let Some(idx) = rest.find(start_marker) {
        let after = &rest[idx + start_marker.len()..];
        let end = after.find(end_marker).expect("closing script tag");
        let raw = &after[..end];
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("valid JSON-LD");
        out.push(parsed);
        rest = &after[end + end_marker.len()..];
    }
    out
}

fn assert_seo_html_hygiene(html: &str, expected_canonical: &str) {
    assert!(!html.contains("style=\""), "CSP forbids inline styles");
    assert!(
        html.contains("index, follow, max-image-preview:large"),
        "missing indexable robots meta"
    );
    assert!(
        html.contains(&format!(
            "<link rel=\"canonical\" href=\"{expected_canonical}\">"
        )),
        "missing canonical {expected_canonical}"
    );
    assert!(html.contains("hreflang=\"pt-BR\""));
    assert!(html.contains("hreflang=\"en\""));
    assert!(html.contains("hreflang=\"es\""));
    assert!(html.contains("hreflang=\"x-default\""));
    assert!(html.contains("property=\"og:image\""));
    assert!(html.contains("name=\"twitter:card\" content=\"summary_large_image\""));
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

async fn get(app: &Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn technical_seo_endpoints_assets_and_dynamic_sitemap() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg.clone(), pool.clone()));

    // 1. robots.txt allows public pages + AI search crawlers and protects private areas
    let robots = get(&app, "/robots.txt").await;
    assert_eq!(robots.status(), StatusCode::OK);
    assert_eq!(robots.headers()[CONTENT_TYPE], "text/plain; charset=utf-8");
    let robots_body = body_str(robots).await;
    assert!(robots_body.contains("Sitemap: https://qrcardapio.com.br/sitemap.xml"));
    assert!(robots_body.contains("Disallow: /dashboard"));
    assert!(robots_body.contains("Disallow: /admin"));
    assert!(robots_body.contains("Disallow: /mcp"));
    for bot in [
        "OAI-SearchBot",
        "PerplexityBot",
        "ClaudeBot",
        "Google-Extended",
    ] {
        assert!(robots_body.contains(bot), "missing AI bot rule {bot}");
    }

    // 2. llms.txt & manifest.webmanifest
    let llms = get(&app, "/llms.txt").await;
    assert_eq!(llms.status(), StatusCode::OK);
    let llms_body = body_str(llms).await;
    assert!(llms_body.contains("https://qrcardapio.com.br/"));
    assert!(llms_body.contains("https://qrcardapio.com.br/cardapio-digital/hamburgueria"));

    let manifest = get(&app, "/manifest.webmanifest").await;
    assert_eq!(manifest.status(), StatusCode::OK);
    let manifest_json: serde_json::Value =
        serde_json::from_slice(&body_bytes(manifest).await).unwrap();
    assert_eq!(manifest_json["name"], "QR Cardápio");

    // 3. Brand icons and localized 1200x630 Open Graph PNGs
    for png_path in [
        "/apple-touch-icon.png",
        "/static/icons/icon-192.png",
        "/static/icons/icon-512.png",
        "/static/icons/icon-maskable-512.png",
        "/static/images/og-pt-BR.png",
        "/static/images/og-en.png",
        "/static/images/og-es.png",
    ] {
        let res = get(&app, png_path).await;
        assert_eq!(res.status(), StatusCode::OK, "asset {png_path}");
        let bytes = body_bytes(res).await;
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "valid PNG {png_path}");
    }

    let ico = get(&app, "/favicon.ico").await;
    assert_eq!(ico.status(), StatusCode::OK);
    let ico_bytes = body_bytes(ico).await;
    assert_eq!(&ico_bytes[..4], &[0, 0, 1, 0]);

    // 4. Dynamic sitemap.xml with hreflang clusters, ETag/304, and published menus
    let sm1 = get(&app, "/sitemap.xml").await;
    assert_eq!(sm1.status(), StatusCode::OK);
    assert_eq!(
        sm1.headers()[CONTENT_TYPE],
        "application/xml; charset=utf-8"
    );
    let etag = sm1.headers()[ETAG].to_str().unwrap().to_string();
    let sm1_xml = body_str(sm1).await;
    assert!(sm1_xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">"));
    assert!(sm1_xml.contains("<loc>https://qrcardapio.com.br/</loc>"));
    assert!(sm1_xml.contains("<loc>https://qrcardapio.com.br/en</loc>"));
    assert!(sm1_xml.contains("<loc>https://qrcardapio.com.br/es</loc>"));
    assert!(sm1_xml.contains(
        "<xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"https://qrcardapio.com.br/\"/>"
    ));
    assert!(sm1_xml.contains("<loc>https://qrcardapio.com.br/cardapio-digital/restaurante</loc>"));
    assert!(
        sm1_xml.contains("<loc>https://qrcardapio.com.br/en/tools/menu-qr-code-generator</loc>")
    );
    assert!(
        sm1_xml
            .contains("<loc>https://qrcardapio.com.br/es/guias/como-crear-menu-digital-qr</loc>")
    );

    let sm_304 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sitemap.xml")
                .header(IF_NONE_MATCH, etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sm_304.status(), StatusCode::NOT_MODIFIED);

    // Publish a menu and confirm /c/cantina-seo enters sitemap.xml dynamically, then disappears on unpublish
    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());
    let locks = PublishLocks::default();
    let auth = register_owner(
        &pool,
        &pw,
        "seo@cantina.com.br",
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
            name: "Cantina SEO",
            slug: "cantina-seo",
            description: "Massas frescas e vinhos.",
            accent_color: "#263be8",
        },
    )
    .await
    .unwrap();
    let cat = create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Massas",
            name_en: "Pasta",
            name_es: "Pastas",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &cat.id,
            name: "Lasanha Bolonhesa",
            name_en: "Beef Lasagna",
            name_es: "Lasaña",
            description: "Assada na hora",
            description_en: "Freshly baked",
            description_es: "Recién horneada",
            price_raw: "42,90",
            is_available: true,
            is_visible: true,
        },
        Some(png_bytes(500, 500)),
    )
    .await
    .unwrap();

    publish_establishment(&pool, &cfg, &locks, &est.id)
        .await
        .unwrap();

    let sm_pub = body_str(get(&app, "/sitemap.xml").await).await;
    assert!(sm_pub.contains("<loc>https://qrcardapio.com.br/c/cantina-seo</loc>"));

    // Published public menu has FoodEstablishment + Menu JSON-LD and canonical
    let menu_html = body_str(get(&app, "/c/cantina-seo").await).await;
    assert!(
        menu_html
            .contains("<link rel=\"canonical\" href=\"https://qrcardapio.com.br/c/cantina-seo\">")
    );
    assert!(menu_html.contains("index, follow"));
    let menu_lds = extract_json_lds(&menu_html);
    assert_eq!(menu_lds.len(), 1);
    let menu_graph = menu_lds[0]["@graph"].as_array().unwrap();
    assert_eq!(menu_graph[0]["@type"], "FoodEstablishment");
    assert_eq!(menu_graph[1]["@type"], "Menu");
    assert_eq!(
        menu_graph[1]["hasMenuSection"][0]["hasMenuItem"][0]["offers"]["price"],
        "42.90"
    );

    let est_latest = find_establishment_for_account(&pool, &auth.account.id)
        .await
        .unwrap()
        .unwrap();
    unpublish_establishment(&pool, &cfg, &locks, &est_latest)
        .await
        .unwrap();
    let sm_unpub = body_str(get(&app, "/sitemap.xml").await).await;
    assert!(!sm_unpub.contains("/c/cantina-seo"));

    pool.close().await;
}

#[tokio::test]
async fn multilingual_landing_segments_tools_guides_and_noindex_pages() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg, pool.clone()));

    // 1. Trailing slash redirects
    for (src, dst) in [("/en/", "/en"), ("/es/", "/es")] {
        let res = get(&app, src).await;
        assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(res.headers()[LOCATION], dst);
    }

    // 2. Multilingual home pages + JSON-LD graph + FAQ + 4-col footer
    for (path, canonical, lang_code) in [
        ("/", "https://qrcardapio.com.br/", "pt-BR"),
        ("/en", "https://qrcardapio.com.br/en", "en"),
        ("/es", "https://qrcardapio.com.br/es", "es"),
        ("/?lang=es", "https://qrcardapio.com.br/es", "es"),
    ] {
        let res = get(&app, path).await;
        assert_eq!(res.status(), StatusCode::OK, "landing {path}");
        assert!(
            res.headers()[CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("public, max-age=300")
        );
        let html = body_str(res).await;
        assert!(html.contains(&format!("<html lang=\"{lang_code}\"")));
        assert_seo_html_hygiene(&html, canonical);
        assert!(html.contains("id=\"faq\""));
        assert!(html.contains("class=\"seo-faq-item\""));
        assert!(html.contains("class=\"landing-footer seo-footer\""));

        let lds = extract_json_lds(&html);
        assert_eq!(lds.len(), 1);
        let graph = lds[0]["@graph"].as_array().unwrap();
        let types: Vec<&str> = graph.iter().filter_map(|n| n["@type"].as_str()).collect();
        for expected in [
            "Organization",
            "WebSite",
            "SoftwareApplication",
            "WebPage",
            "HowTo",
            "FAQPage",
        ] {
            assert!(types.contains(&expected), "missing schema type {expected}");
        }
    }

    // 3. Auth pages emit noindex, nofollow
    for auth_path in ["/login", "/register"] {
        let html = body_str(get(&app, auth_path).await).await;
        assert!(
            html.contains("<meta name=\"robots\" content=\"noindex, nofollow\">"),
            "auth page {auth_path} must be noindex"
        );
        assert!(!html.contains("application/ld+json"));
    }

    // 4. All 8 segments x 3 locales + hubs
    for (hub_path, locale) in [
        ("/solucoes", Locale::PtBr),
        ("/en/solutions", Locale::En),
        ("/es/soluciones", Locale::Es),
    ] {
        let html = body_str(get(&app, hub_path).await).await;
        assert_seo_html_hygiene(&html, &format!("https://qrcardapio.com.br{hub_path}"));
        for seg_id in SegmentId::ALL {
            let p = seg_id.full_path(locale);
            let seg_html = body_str(get(&app, p).await).await;
            assert_seo_html_hygiene(&seg_html, &format!("https://qrcardapio.com.br{p}"));
            let lds = extract_json_lds(&seg_html);
            assert_eq!(lds.len(), 1);
            let types: Vec<&str> = lds[0]["@graph"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|n| n["@type"].as_str())
                .collect();
            assert!(types.contains(&"BreadcrumbList"));
            assert!(types.contains(&"FAQPage"));
        }
    }

    // 5. All 3 tools x 3 locales + QR API endpoints
    for (hub_path, locale) in [
        ("/ferramentas", Locale::PtBr),
        ("/en/tools", Locale::En),
        ("/es/herramientas", Locale::Es),
    ] {
        let html = body_str(get(&app, hub_path).await).await;
        assert_seo_html_hygiene(&html, &format!("https://qrcardapio.com.br{hub_path}"));
        for tool_id in ToolId::ALL {
            let p = tool_id.full_path(locale);
            let tool_html = body_str(get(&app, p).await).await;
            assert_seo_html_hygiene(&tool_html, &format!("https://qrcardapio.com.br{p}"));
            let lds = extract_json_lds(&tool_html);
            assert_eq!(lds.len(), 1);
            let types: Vec<&str> = lds[0]["@graph"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|n| n["@type"].as_str())
                .collect();
            assert!(types.contains(&"WebApplication"));
        }
    }

    let svg_res = get(
        &app,
        "/api/tools/qr.svg?url=https%3A%2F%2Fqrcardapio.com.br",
    )
    .await;
    assert_eq!(svg_res.status(), StatusCode::OK);
    assert!(body_str(svg_res).await.contains("<svg"));

    let png_res = get(
        &app,
        "/api/tools/qr.png?url=https%3A%2F%2Fqrcardapio.com.br",
    )
    .await;
    assert_eq!(png_res.status(), StatusCode::OK);
    assert_eq!(&body_bytes(png_res).await[..8], b"\x89PNG\r\n\x1a\n");

    let bad_qr = get(&app, "/api/tools/qr.svg?url=javascript%3Aalert(1)").await;
    assert_eq!(bad_qr.status(), StatusCode::BAD_REQUEST);

    // 6. All 4 guides x 3 locales + comparison table + 404 unknown slugs
    for (hub_path, locale) in [
        ("/guias", Locale::PtBr),
        ("/en/guides", Locale::En),
        ("/es/guias", Locale::Es),
    ] {
        let html = body_str(get(&app, hub_path).await).await;
        assert_seo_html_hygiene(&html, &format!("https://qrcardapio.com.br{hub_path}"));
        for guide_id in GuideId::ALL {
            let p = guide_id.full_path(locale);
            let guide_html = body_str(get(&app, p).await).await;
            assert_seo_html_hygiene(&guide_html, &format!("https://qrcardapio.com.br{p}"));
            if guide_id == GuideId::PdfVsDigital {
                assert!(guide_html.contains("class=\"seo-table\""));
            }
            let lds = extract_json_lds(&guide_html);
            assert_eq!(lds.len(), 1);
            let types: Vec<&str> = lds[0]["@graph"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|n| n["@type"].as_str())
                .collect();
            assert!(types.contains(&"Article"));
            assert!(types.contains(&"BreadcrumbList"));
            assert!(types.contains(&"FAQPage"));
        }
    }

    assert_eq!(
        get(&app, "/cardapio-digital/inexistente").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/ferramentas/inexistente").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/guias/inexistente").await.status(),
        StatusCode::NOT_FOUND
    );

    // Language cookie persists explicit locale on /
    let cookie_landing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(COOKIE, "qr_lang=en")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cookie_landing.status(), StatusCode::OK);
    assert!(body_str(cookie_landing).await.contains("<html lang=\"en\""));

    // Local preview landing emits all 3 BillingPlan offer cycles in SoftwareApplication JSON-LD
    let mut local_cfg = test_config(&TempDir::new().unwrap());
    local_cfg.public_base_url = "http://localhost:8080".to_string();
    let local_app = build_router(AppState::new(local_cfg, pool.clone()));
    let local_html = body_str(get(&local_app, "/").await).await;
    let local_lds = extract_json_lds(&local_html);
    let sw = local_lds[0]["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["@type"] == "SoftwareApplication")
        .unwrap();
    let offers = sw["offers"].as_array().unwrap();
    assert_eq!(offers.len(), 3);
    assert_eq!(offers[0]["price"], "29.90");

    pool.close().await;
}
