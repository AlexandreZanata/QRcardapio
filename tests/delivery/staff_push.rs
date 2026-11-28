use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::register_owner;
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::security::PasswordService;

use crate::fixture::{Shop, body, expect_json, get_public, grant_preview};
use crate::support::{config, get, harness, manager, owner};

const ORDERS: &str = "/dashboard/delivery/orders";
const MANIFEST: &str = "/dashboard/delivery/manifest.webmanifest";
const SUBSCRIPTIONS: &str = "/dashboard/delivery/push-subscriptions";
const WORKER: &str = "/dashboard/delivery/sw.js";

#[tokio::test]
async fn staff_install_manifest_requires_enabled_push_and_a_preview_grant() {
    let h = harness(false).await;
    let shop = Shop::create(&h, "delivery-staff-install", 0).await;
    let (other, _) = owner(&h.pool, "delivery-staff-no-access").await;
    grant_preview(&h, &shop).await;

    let response = get(&h.app, &shop.owner, ORDERS).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!body(response).await.contains(MANIFEST));
    assert_eq!(
        get(&h.app, &shop.owner, MANIFEST).await.status(),
        StatusCode::NOT_FOUND
    );

    let mut state = AppState::new(config(&h.dir, false), h.pool.clone());
    state.staff_push_public_key = Some("synthetic-public-key".into());
    let app = build_router(state);

    let guest = get_public(&app, MANIFEST, None).await;
    assert_eq!(guest.status(), StatusCode::SEE_OTHER);
    assert_eq!(guest.headers()[header::LOCATION], "/login");
    assert_eq!(
        get(&app, &other, MANIFEST).await.status(),
        StatusCode::FORBIDDEN
    );

    let response = get(&app, &shop.owner, &format!("{ORDERS}?lang=en")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let page = body(response).await;
    assert!(page.contains(&format!(
        "rel=\"manifest\" href=\"{MANIFEST}?lang=en\" crossorigin=\"use-credentials\""
    )));

    let response = get(&app, &shop.owner, &format!("{MANIFEST}?lang=en")).await;
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/manifest+json; charset=utf-8"
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let manifest = expect_json(response, StatusCode::OK).await;
    assert_eq!(manifest["id"], ORDERS);
    assert_eq!(manifest["start_url"], ORDERS);
    assert_eq!(manifest["scope"], "/dashboard/delivery/");
    assert_eq!(manifest["display"], "standalone");
    assert_eq!(manifest["lang"], "en");
    assert_eq!(manifest["short_name"], "Order Center");

    let worker = get(&app, &shop.owner, WORKER).await;
    assert_eq!(worker.status(), StatusCode::OK);
    assert_eq!(worker.headers()[header::CACHE_CONTROL], "no-store");
    assert!(!worker.headers().contains_key("service-worker-allowed"));
    let source = body(worker).await;
    assert!(source.contains("addEventListener(\"push\""));
    assert!(!source.contains("addEventListener(\"fetch\""));
}

#[tokio::test]
async fn push_registration_and_installation_require_a_supported_staff_role() {
    let h = harness(false).await;
    let shop = Shop::create(&h, "delivery-staff-push-roles", 0).await;
    grant_preview(&h, &shop).await;
    let manager = manager(&h.pool, &shop.id, "delivery-push-manager@example.test").await;
    let mut state = AppState::new(config(&h.dir, false), h.pool.clone());
    state.staff_push_public_key = Some("synthetic-public-key".into());
    let app = build_router(state);
    assert_eq!(get(&app, &manager, MANIFEST).await.status(), StatusCode::OK);

    let password_service = PasswordService::new();
    for role in ["cashier", "waiter"] {
        let member = register_owner(
            &h.pool,
            &password_service,
            &format!("delivery-push-{role}@example.test"),
            "TestPassword123",
            "TestPassword123",
            Locale::PtBr,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO establishment_member_roles (establishment_id, account_id, member_role, created_at)
             VALUES (?, ?, ?, datetime('now'))",
        )
        .bind(&shop.id)
        .bind(&member.account.id)
        .bind(role)
        .execute(&h.pool)
        .await
        .unwrap();

        let orders = get(&app, &member, ORDERS).await;
        assert_eq!(orders.status(), StatusCode::OK);
        let page = body(orders).await;
        assert!(page.contains("data-delivery-sound"));
        assert!(!page.contains("data-delivery-push"));
        assert!(!page.contains(MANIFEST));
        assert_eq!(
            get(&app, &member, MANIFEST).await.status(),
            StatusCode::FORBIDDEN
        );

        let response = app
            .clone()
            .oneshot(
                Request::post(SUBSCRIPTIONS)
                    .header(header::COOKIE, format!("qr_session={}", member.raw_session_token))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"_csrf": member.csrf_token, "endpoint": "", "p256dh": "", "auth": ""})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
