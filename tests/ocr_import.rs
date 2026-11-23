use std::io::Cursor;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use image::{ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    find_establishment_for_account, list_categories_for_est, list_products_for_est,
};

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

async fn body_string(res: axum::response::Response) -> String {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
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

/// Builds a standards-compliant single-page PDF 1.4 containing the given ASCII text lines.
fn build_minimal_pdf(lines: &[&str]) -> Vec<u8> {
    let mut stream = String::from("BT\n/F1 12 Tf\n16 TL\n72 740 Td\n");
    for (i, line) in lines.iter().enumerate() {
        let escaped = line
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        if i == 0 {
            stream.push_str(&format!("({escaped}) Tj\n"));
        } else {
            stream.push_str(&format!("T* ({escaped}) Tj\n"));
        }
    }
    stream.push_str("ET\n");

    let objects = [
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n".to_string(),
        "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
        format!(
            "5 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
            stream.len(),
            stream
        ),
    ];

    let mut pdf = Vec::from(b"%PDF-1.4\n".as_slice());
    let mut offsets = Vec::with_capacity(objects.len());
    for obj in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(obj.as_bytes());
    }

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// Generates a clean white PNG with crisp dark text-like strokes suitable for Tesseract pipeline verification.
fn sample_menu_png() -> Vec<u8> {
    let mut img = RgbaImage::from_pixel(420, 140, Rgba([255, 255, 255, 255]));
    let black = Rgba([15, 15, 15, 255]);
    // Draw a crisp high-contrast horizontal menu stripe / digit shapes so OCR runs cleanly on real pixels.
    for x in 40..380 {
        for y in 40..44 {
            img.put_pixel(x, y, black);
        }
        for y in 96..100 {
            img.put_pixel(x, y, black);
        }
    }
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out))
        .write_image(
            &img,
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    out
}

async fn register_and_onboard(
    app: &axum::Router,
    email: &str,
    est_name: &str,
    slug: &str,
) -> (String, String) {
    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "email={email}&password=SenhaForte123&password_confirm=SenhaForte123&website="
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::SEE_OTHER);
    let cookie = extract_session_cookie(&reg).unwrap();

    let onb_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/onboarding")
                .header(COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csrf = extract_csrf_token(&body_string(onb_get).await);

    let onb_post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/onboarding")
                .header(COOKIE, &cookie)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf}&name={est_name}&slug={slug}&description=Cardapio&accent_color=%23D9532F"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(onb_post.status(), StatusCode::SEE_OTHER);
    (cookie, csrf)
}

#[tokio::test]
async fn digital_pdf_ocr_review_and_bulk_apply_flow() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg.clone(), pool.clone()));

    let (cookie, csrf) =
        register_and_onboard(&app, "pdf@bistro.com.br", "Bistro", "bistro-ocr").await;

    // 1. Upload a real digital PDF containing categories and prices.
    let pdf = build_minimal_pdf(&[
        "BEBIDAS",
        "Espresso Duplo ............. R$ 9,50",
        "Cappuccino Cremoso ......... R$ 14,90",
        "SALGADOS",
        "Pao de Queijo Mineiro ...... R$ 8,00",
    ]);
    let boundary = "----QRCardapioPdfBoundary";
    let body = build_multipart_body(
        boundary,
        &[("_csrf", &csrf)],
        Some(("menu_file", "cardapio-2026.pdf", "application/pdf", &pdf)),
    );

    let extract_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/ocr/extract")
                .header(COOKIE, &cookie)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(extract_res.status(), StatusCode::SEE_OTHER);
    let review_loc = extract_res
        .headers()
        .get(LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(review_loc.starts_with("/dashboard/ocr/"));

    // 2. Load review page and confirm extracted items appear pre-filled.
    let review_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&review_loc)
                .header(COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(review_res.status(), StatusCode::OK);
    let review_html = body_string(review_res).await;
    assert!(review_html.contains("Espresso Duplo"));
    assert!(review_html.contains("9,50"));
    assert!(review_html.contains("Cappuccino Cremoso"));
    assert!(review_html.contains("14,90"));
    assert!(review_html.contains("Pao de Queijo Mineiro"));
    assert!(review_html.contains("ocr.source_pdf_text"));

    // 3. Apply reviewed items (editing one name and deselecting item 1).
    let apply_body = format!(
        "_csrf={csrf}\
        &reuse_existing_categories=1\
        &mark_visible=1\
        &row_idx=0&include_0=1&category_0=Bebidas&name_0=Espresso+Especial&price_0=10%2C50&description_0=Graos+selecionados&available_0=1\
        &row_idx=1&category_1=Bebidas&name_1=Cappuccino+Cremoso&price_1=14%2C90&description_1=&available_1=1\
        &row_idx=2&include_2=1&category_2=Salgados&name_2=Pao+de+Queijo&price_2=8%2C00&description_2=Quentinho&available_2=1"
    );
    let apply_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{review_loc}/apply"))
                .header(COOKIE, &cookie)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(apply_body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(apply_res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        apply_res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/dashboard/menu"
    );

    // 4. Verify database draft state (exact centavos, categories, unpublished flag).
    let est_id: String =
        sqlx::query_scalar("SELECT id FROM establishments WHERE slug = 'bistro-ocr'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let cats = list_categories_for_est(&pool, &est_id).await.unwrap();
    assert_eq!(cats.len(), 2);
    let prods = list_products_for_est(&pool, &est_id).await.unwrap();
    assert_eq!(prods.len(), 2);
    assert!(
        prods
            .iter()
            .any(|p| p.name == "Espresso Especial" && p.price_centavos == 1050)
    );
    assert!(
        prods
            .iter()
            .any(|p| p.name == "Pao de Queijo" && p.price_centavos == 800)
    );

    let account_id: String =
        sqlx::query_scalar("SELECT id FROM accounts WHERE email = 'pdf@bistro.com.br'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let est = find_establishment_for_account(&pool, &account_id)
        .await
        .unwrap()
        .unwrap();
    assert!(est.has_unpublished_changes);
    assert!(!est.is_published);

    // 5. Re-applying the same import must fail with 409 Conflict.
    let reapply = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{review_loc}/apply"))
                .header(COOKIE, &cookie)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(apply_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reapply.status(), StatusCode::CONFLICT);

    // 6. Second import with reuse_existing_categories=1 reuses "Bebidas" case-insensitively.
    let second_text = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/ocr/text")
                .header(COOKIE, &cookie)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf}&raw_text=bebidas%0ACha+Gelado+R%24+11%2C00"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_text.status(), StatusCode::SEE_OTHER);
    let second_loc = second_text
        .headers()
        .get(LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let second_apply = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{second_loc}/apply"))
                .header(COOKIE, &cookie)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf}&reuse_existing_categories=1&mark_visible=1&row_idx=0&include_0=1&category_0=bebidas&name_0=Cha+Gelado&price_0=11%2C00&description_0=&available_0=1"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_apply.status(), StatusCode::SEE_OTHER);
    let cats_after = list_categories_for_est(&pool, &est_id).await.unwrap();
    assert_eq!(cats_after.len(), 2);
    let prods_after = list_products_for_est(&pool, &est_id).await.unwrap();
    assert_eq!(prods_after.len(), 3);
}

#[tokio::test]
async fn image_ocr_pasted_text_validation_xss_and_tenant_isolation() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg.clone(), pool.clone()));

    let (cookie_a, csrf_a) =
        register_and_onboard(&app, "a@ocr.com.br", "Casa A", "casa-a-ocr").await;
    let (cookie_b, csrf_b) =
        register_and_onboard(&app, "b@ocr.com.br", "Casa B", "casa-b-ocr").await;

    // 1. Real PNG image upload runs preprocessing + local Tesseract end-to-end.
    let png = sample_menu_png();
    let boundary = "----QRCardapioPngBoundary";
    let png_body = build_multipart_body(
        boundary,
        &[("_csrf", &csrf_a)],
        Some(("menu_file", "foto.png", "image/png", &png)),
    );
    let img_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/ocr/extract")
                .header(COOKIE, &cookie_a)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(png_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(img_res.status(), StatusCode::SEE_OTHER);
    let img_loc = img_res
        .headers()
        .get(LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let img_review = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&img_loc)
                .header(COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(img_review.status(), StatusCode::OK);
    let img_html = body_string(img_review).await;
    assert!(img_html.contains("ocr.source_image"));

    // 2. SVG disguised as PDF must be rejected with 400 (magic-byte defense).
    let svg_body = build_multipart_body(
        boundary,
        &[("_csrf", &csrf_a)],
        Some((
            "menu_file",
            "evil.pdf",
            "application/pdf",
            b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>",
        )),
    );
    let svg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/ocr/extract")
                .header(COOKIE, &cookie_a)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(svg_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(svg_res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body_string(svg_res)
            .await
            .contains("error.ocr_unsupported_file")
    );

    // 3. Pasted text with XSS payload is properly escaped in review page.
    let paste_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dashboard/ocr/text")
                .header(COOKIE, &cookie_a)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf_a}&raw_text=LANCHES%0A%3Cscript%3Ealert(1)%3C%2Fscript%3E+Burger+R%24+29%2C90"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paste_res.status(), StatusCode::SEE_OTHER);
    let paste_loc = paste_res
        .headers()
        .get(LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let paste_review = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&paste_loc)
                .header(COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paste_review.status(), StatusCode::OK);
    let paste_html = body_string(paste_review).await;
    assert!(!paste_html.contains("<script>alert(1)</script>"));
    assert!(paste_html.contains("29,90"));

    // 4. Tenant B cannot view, apply, or discard Tenant A's OCR import.
    let cross_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&paste_loc)
                .header(COOKIE, &cookie_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_get.status(), StatusCode::NOT_FOUND);

    let cross_discard = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{paste_loc}/discard"))
                .header(COOKIE, &cookie_b)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("_csrf={csrf_b}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_discard.status(), StatusCode::NOT_FOUND);

    // 5. Missing CSRF is rejected with 403 Forbidden.
    let bad_csrf = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{paste_loc}/discard"))
                .header(COOKIE, &cookie_a)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("_csrf=wrong-token"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_csrf.status(), StatusCode::FORBIDDEN);

    // 6. Invalid price on apply returns 400 and preserves edited row values.
    let bad_price = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{paste_loc}/apply"))
                .header(COOKIE, &cookie_a)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "_csrf={csrf_a}&row_idx=0&include_0=1&category_0=Lanches&name_0=X-Bacon&price_0=abc&description_0=&available_0=1"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_price.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bad_html = body_string(bad_price).await;
    assert!(bad_html.contains("error.invalid_price"));
    assert!(bad_html.contains("X-Bacon"));

    // 7. Owner A can discard their own import cleanly.
    let discard_ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{paste_loc}/discard"))
                .header(COOKIE, &cookie_a)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("_csrf={csrf_a}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discard_ok.status(), StatusCode::SEE_OTHER);
}
