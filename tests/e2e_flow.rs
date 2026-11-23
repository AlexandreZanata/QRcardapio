use std::io::Cursor;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use image::{DynamicImage, ImageEncoder, codecs::png::PngEncoder};
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::config::{AsaasConfig, AsaasEnvironment, Config, DbSynchronous, LogFormat};
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::rendering::billing::plan_price;

fn test_config(dir: &TempDir) -> Config {
    Config {
        asaas: None,
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

fn extract_session_cookie(res: &axum::response::Response) -> Option<String> {
    for val in res.headers().get_all(SET_COOKIE) {
        if let Ok(s) = val.to_str()
            && let Some(rest) = s.strip_prefix("qr_session=")
            && let Some((tok, _)) = rest.split_once(';')
            && !tok.is_empty()
        {
            return Some(format!("qr_session={tok}"));
        }
    }
    None
}

fn extract_csrf_token(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("missing _csrf hidden input") + marker.len();
    let end = html[start..].find('"').expect("unterminated _csrf value");
    html[start..start + end].to_string()
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

fn build_multipart_body(
    boundary: &str,
    fields: &[(&str, &str)],
    file_field: Option<(&str, &str, &str, &[u8])>,
) -> Vec<u8> {
    let mut body = Vec::new();
    for (k, v) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes(),
        );
    }
    if let Some((field_name, filename, mime, bytes)) = file_field {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

async fn body_string(res: axum::response::Response) -> String {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn landing_plan_reaches_billing_after_registration_and_login() {
    let dir = TempDir::new().unwrap();
    let mut cfg = test_config(&dir);
    cfg.asaas = Some(
        AsaasConfig::new(
            AsaasEnvironment::Sandbox,
            "test-api-credential",
            &"a".repeat(40),
        )
        .unwrap(),
    );
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg, pool));
    let send = |path: &str, cookie: &str, form: Option<String>| {
        let mut request = Request::builder().uri(path).header(COOKIE, cookie);
        let body = if let Some(form) = form {
            request = request
                .method("POST")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(form)
        } else {
            Body::empty()
        };
        app.clone().oneshot(request.body(body).unwrap())
    };

    let landing = body_string(send("/", "", None).await.unwrap()).await;
    assert!(landing.contains("id=\"landing-plan-dialog\""));
    assert!(landing.contains(&plan_price()));
    assert!(landing.contains("href=\"/register?plan=1\""));
    assert!(landing.contains("href=\"/login?plan=1\""));

    let account = "email=plano%40example.test&password=SenhaForte123";
    let register = send(
        "/register?plan=1",
        "",
        Some(format!("{account}&password_confirm=SenhaForte123")),
    )
    .await
    .unwrap();
    let onboarding_url = "/onboarding?plan=1";
    assert_eq!(register.headers()[LOCATION], onboarding_url);
    let cookie = extract_session_cookie(&register).unwrap();
    let onboarding = body_string(send(onboarding_url, &cookie, None).await.unwrap()).await;
    assert!(onboarding.contains("action=\"/onboarding?plan=1\""));
    let csrf = extract_csrf_token(&onboarding);
    let created = send(
        onboarding_url,
        &cookie,
        Some(format!(
            "_csrf={csrf}&name=Plano+Teste&slug=plano-teste&accent_color=%23D9532F"
        )),
    )
    .await
    .unwrap();
    let billing_url = "/dashboard/billing";
    assert_eq!(created.headers()[LOCATION], billing_url);

    let login = send("/login?plan=1", "", Some(account.into()))
        .await
        .unwrap();
    assert_eq!(login.headers()[LOCATION], onboarding_url);
    let cookie = extract_session_cookie(&login).unwrap();
    let redirect = send(onboarding_url, &cookie, None).await.unwrap();
    assert_eq!(redirect.headers()[LOCATION], billing_url);
    let billing = body_string(send(billing_url, &cookie, None).await.unwrap()).await;
    assert!(billing.contains(&plan_price()));
    assert!(billing.contains("action=\"/dashboard/billing/checkout\""));
}

#[tokio::test]
async fn complete_definition_of_done_e2e_flow_including_restart() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);

    // Boot 1
    let pool1 = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg.clone(), pool1.clone()));

    // 1. Landing page renders cleanly
    let landing = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(landing.status(), StatusCode::OK);
    let landing_html = body_string(landing).await;
    assert!(landing_html.contains("Seu cardápio."));
    assert!(landing_html.contains("href=\"#how-it-works\""));
    assert!(landing_html.contains("/static/images/landing-dashboard.webp"));
    assert!(landing_html.contains("/static/images/landing-review-frame-4k.webp 2160w"));
    assert!(!landing_html.contains("href=\"/static/images/landing-dashboard-4k.webp\""));
    assert!(!landing_html.contains("href=\"/static/images/landing-menu-4k.webp\""));
    assert!(!landing_html.contains("id=\"landing-plan-dialog\""));

    // 2. Register Owner 1
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=owner1%40bistro.com.br&password=SenhaForte123&password_confirm=SenhaForte123",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::SEE_OTHER);
    let cookie1 = extract_session_cookie(&reg_res).expect("session cookie");

    // 3. Onboarding page -> obtain CSRF and create establishment
    let onb_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/onboarding")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(onb_get.status(), StatusCode::OK);
    let csrf1 = extract_csrf_token(&body_string(onb_get).await);

    let onb_post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/onboarding")
                .header(COOKIE, &cookie1)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf1}&name=Bistr%C3%B4+do+Sol&slug=bistro-do-sol&description=Comida+caseira&accent_color=%23D9532F"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(onb_post.status(), StatusCode::SEE_OTHER);
    assert_eq!(onb_post.headers().get(LOCATION).unwrap(), "/dashboard/menu");

    let settings = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/settings")
                .header(COOKIE, &cookie1)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf1}&name=Bistr%C3%B4+do+Sol&slug=bistro-do-sol&description=Comida+caseira&accent_color=%23D9532F&google_maps_url=https%3A%2F%2Fmaps.app.goo.gl%2Fexample&google_review_url=https%3A%2F%2Fg.page%2Fr%2Fexample%2Freview"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settings.status(), StatusCode::SEE_OTHER);

    // 4. Upload logo via multipart
    let boundary = "qrboundary123";
    let logo_body = build_multipart_body(
        boundary,
        &[("_csrf", &csrf1)],
        Some(("logo", "logo.png", "image/png", &png_bytes(200, 200))),
    );
    let logo_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/settings/logo")
                .header(COOKIE, &cookie1)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(logo_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logo_res.status(), StatusCode::SEE_OTHER);

    // 5. Create 3 categories
    for cname in ["Entradas", "Pratos Principais", "Sobremesas"] {
        let cat_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/dashboard/categories")
                    .header(COOKIE, &cookie1)
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "_csrf={csrf1}&name={cname}&is_visible=1"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cat_res.status(), StatusCode::SEE_OTHER);
    }

    // Read category ID from menu editor
    let menu_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard/menu")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let menu_html = body_string(menu_res).await;
    let cat_marker = "action=\"/dashboard/categories/";
    let c_start = menu_html.find(cat_marker).unwrap() + cat_marker.len();
    let c_end = menu_html[c_start..].find('/').unwrap();
    let cat_id = &menu_html[c_start..c_start + c_end];

    // 6. Add product with image and price 24,90
    let prod_body = build_multipart_body(
        boundary,
        &[
            ("_csrf", &csrf1),
            ("category_id", cat_id),
            ("name", "Bruschetta Italiana"),
            ("name_en", "Italian Bruschetta"),
            ("name_es", "Bruschetta Italiana"),
            ("price", "24,90"),
            ("description", "Tomate fresco e manjericão"),
            ("is_available", "1"),
            ("is_visible", "1"),
        ],
        Some(("image", "dish.png", "image/png", &png_bytes(600, 450))),
    );
    let prod_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/products")
                .header(COOKIE, &cookie1)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(prod_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(prod_res.status(), StatusCode::SEE_OTHER);

    // 7. Draft preview works before publishing
    let preview = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard/preview")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    let preview_html = body_string(preview).await;
    assert!(preview_html.contains("Modo prévia"));
    assert!(preview_html.contains("Bruschetta Italiana"));
    assert!(preview_html.contains("R$ 24,90"));

    // 8. Publish menu
    let pub_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/publish")
                .header(COOKIE, &cookie1)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("_csrf={csrf1}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pub_res.status(), StatusCode::SEE_OTHER);

    let settings_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard/settings")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settings_page.status(), StatusCode::OK);
    let settings_html = body_string(settings_page).await;
    assert!(settings_html.contains("class=\"dashboard-address dashboard-address-locked\""));
    assert!(settings_html.contains("readonly"));
    assert!(settings_html.contains("value=\"https://maps.app.goo.gl/example\""));
    assert!(settings_html.contains("value=\"https://g.page/r/example/review\""));
    assert!(settings_html.contains("type=\"file\" id=\"logo\" name=\"logo\""));
    assert_eq!(
        settings_html
            .matches("id=\"dashboard-file-picker\"")
            .count(),
        1
    );

    // 9. Public URL serves menu
    let public_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/c/bistro-do-sol")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public_res.status(), StatusCode::OK);
    let public_html = body_string(public_res).await;
    assert!(public_html.contains("Bistrô do Sol"));
    assert!(public_html.contains("R$ 24,90"));
    assert!(public_html.contains("href=\"https://g.page/r/example/review\""));
    assert!(public_html.contains("Avalie-nos no Google Maps"));

    let restricted = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restricted.status(), StatusCode::FORBIDDEN);
    assert!(
        body_string(restricted)
            .await
            .contains("state-forbidden.svg")
    );

    // 10. QR SVG, PNG, and Print endpoints work
    let qr_svg = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard/qr.svg")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(qr_svg.status(), StatusCode::OK);
    assert!(body_string(qr_svg).await.contains("<svg"));

    let qr_png = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard/qr.png")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(qr_png.status(), StatusCode::OK);
    let png_data = qr_png.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&png_data[..8], b"\x89PNG\r\n\x1a\n");

    let qr_print = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard/qr/print")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(qr_print.status(), StatusCode::OK);
    assert!(body_string(qr_print).await.contains("Bistrô do Sol"));

    // 11. Find product ID, update price to 29,90, and republish
    let menu2_html = body_string(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/dashboard/menu")
                    .header(COOKIE, &cookie1)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    let prod_marker = "action=\"/dashboard/products/";
    let p_start = menu2_html.find(prod_marker).unwrap() + prod_marker.len();
    let p_end = menu2_html[p_start..].find('/').unwrap();
    let prod_id = &menu2_html[p_start..p_start + p_end];

    let update_body = build_multipart_body(
        boundary,
        &[
            ("_csrf", &csrf1),
            ("category_id", cat_id),
            ("name", "Bruschetta Italiana"),
            ("price", "29,90"),
            ("description", "Tomate fresco e manjericão"),
            ("is_available", "1"),
            ("is_visible", "1"),
        ],
        None,
    );
    let upd_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/dashboard/products/{prod_id}"))
                .header(COOKIE, &cookie1)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upd_res.status(), StatusCode::SEE_OTHER);

    let repub = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/publish")
                .header(COOKIE, &cookie1)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("_csrf={csrf1}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repub.status(), StatusCode::SEE_OTHER);

    let public_v2 = body_string(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/c/bistro-do-sol")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert!(public_v2.contains("R$ 29,90"));

    // 12. Register Owner 2 -> verify cannot edit/delete Owner 1's category or product via HTTP
    let reg2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=owner2%40outro.com.br&password=SenhaForte123&password_confirm=SenhaForte123",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie2 = extract_session_cookie(&reg2).unwrap();
    let onb2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/onboarding")
                .header(COOKIE, &cookie2)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csrf2 = extract_csrf_token(&body_string(onb2).await);
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/onboarding")
                .header(COOKIE, &cookie2)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf2}&name=Outro+Lugar&slug=outro-lugar&accent_color=%231D4ED8"
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    let idor_cat = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/dashboard/categories/{cat_id}/edit"))
                .header(COOKIE, &cookie2)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_cat.status(), StatusCode::NOT_FOUND);

    let idor_prod_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/dashboard/products/{prod_id}/delete"))
                .header(COOKIE, &cookie2)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("_csrf={csrf2}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idor_prod_del.status(), StatusCode::NOT_FOUND);

    // 13. Simulated server restart: close pool1, reopen fresh pool2 + router
    drop(app);
    pool1.close().await;

    let pool2 = init_pool(&cfg).await.unwrap();
    let app2 = build_router(AppState::new(cfg, pool2.clone()));

    let after_restart_dash = app2
        .clone()
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .header(COOKIE, &cookie1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_restart_dash.status(), StatusCode::OK);
    let dash_html = body_string(after_restart_dash).await;
    assert!(dash_html.contains("Bistrô do Sol"));
    assert!(dash_html.contains("v2"));
    assert!(dash_html.contains("aria-controls=\"dashboard-navigation\""));
    assert!(dash_html.contains("href=\"/dashboard\" aria-current=\"page\""));
    assert!(dash_html.contains("class=\"panel-publication-actions\""));
    assert!(dash_html.contains("Publicar atualizações"));
    assert!(dash_html.contains("href=\"/dashboard/preview\""));
    assert!(dash_html.contains("href=\"/c/bistro-do-sol\""));
    assert!(!dash_html.contains("href=\"/admin\""));

    let after_restart_pub = app2
        .oneshot(
            Request::builder()
                .uri("/c/bistro-do-sol")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_restart_pub.status(), StatusCode::OK);
    assert!(body_string(after_restart_pub).await.contains("R$ 29,90"));

    pool2.close().await;
}
