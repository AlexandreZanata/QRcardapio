use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::{AuthSuccess, register_owner};
use qrcardapio::application::establishments::{CreateEstablishmentInput, create_establishment};
use qrcardapio::config::{AsaasConfig, AsaasEnvironment, Config, DbSynchronous, LogFormat};
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::database::{init_pool, queries::billing};
use qrcardapio::infrastructure::security::PasswordService;

fn webhook_token() -> String {
    "a".repeat(40)
}

async fn owner(pool: &SqlitePool, slug: &str) -> (AuthSuccess, String) {
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
            name: "QR Test",
            slug,
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();
    (login, est.id)
}

async fn webhook(app: &Router, token: Option<&str>, event: Value) -> StatusCode {
    let mut request =
        Request::post("/webhooks/asaas").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header("asaas-access-token", token);
    }
    app.clone()
        .oneshot(request.body(Body::from(event.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

fn auth_request(path: &str, login: &AuthSuccess) -> axum::http::request::Builder {
    Request::builder().uri(path).header(
        header::COOKIE,
        format!("qr_session={}", login.raw_session_token),
    )
}

async fn post_form(
    app: &Router,
    login: &AuthSuccess,
    path: &str,
    form: String,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            auth_request(path, login)
                .method("POST")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap()
}

fn test_config(dir: &TempDir, token: &str) -> Config {
    Config {
        mcp_enabled: false,
        asaas: Some(
            AsaasConfig::new(AsaasEnvironment::Sandbox, "test-api-credential", token).unwrap(),
        ),
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

#[tokio::test]
async fn billing_is_tenant_scoped_and_tracks_authenticated_financial_events() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));
    let (alice, est) = owner(&pool, "test-billing-a").await;
    let (bob, other_est) = owner(&pool, "test-billing-b").await;
    let today = billing_now().date_naive();
    let first = Uuid::new_v4().to_string();
    let second = Uuid::new_v4().to_string();
    let (a, b) = tokio::join!(
        billing::reserve_checkout(&pool, "sandbox", &est, &first, today, BillingPlan::Monthly),
        billing::reserve_checkout(&pool, "sandbox", &est, &second, today, BillingPlan::Monthly),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a, b);
    let reference = if a { first } else { second };
    let id = Uuid::new_v4();
    let created = json!({"id":"evt_created","event":"CHECKOUT_CREATED","checkout":{
        "id": id.to_string(), "callback":{"successUrl":format!("https://menu.example.test/dashboard/billing?checkout={reference}")}
    }});
    assert_eq!(
        webhook(&app, None, created.clone()).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(webhook(&app, Some(&token), created).await, StatusCode::OK);

    let res = app
        .clone()
        .oneshot(
            auth_request("/dashboard/billing", &alice)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains(&format!(
        "https://sandbox.asaas.com/checkoutSession/show/{id}"
    )));
    let res = post_form(
        &app,
        &alice,
        "/dashboard/billing/checkout",
        "_csrf=invalid".into(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let res = post_form(
        &app,
        &alice,
        "/dashboard/billing/checkout",
        format!("_csrf={}", alice.csrf_token),
    )
    .await;
    assert_eq!(res.headers()[header::LOCATION], "/dashboard/billing");

    let checkout_paid = json!({"id":"evt_checkout_paid", "event":"CHECKOUT_PAID", "checkout":{"id":id.to_string()}});
    assert_eq!(
        webhook(&app, Some(&token), checkout_paid).await,
        StatusCode::OK
    );
    let publish = |login: &AuthSuccess| format!("_csrf={}", login.csrf_token);
    let res = post_form(&app, &alice, "/dashboard/publish", publish(&alice)).await;
    assert_eq!(res.headers()[header::LOCATION], "/dashboard/billing");

    let payment = |event_id: &str, event: &str, time: &str, value: Value| {
        json!({
            "id": event_id, "event":event, "dateCreated":format!("{today} {time}"), "payment":{
                "id":"pay_test", "subscription":"sub_test", "dueDate":today.to_string(), "value":value
            }
        })
    };
    let wrong_amount = payment("evt_wrong", "PAYMENT_CONFIRMED", "10:00:00", json!(1));
    assert_eq!(
        webhook(&app, Some(&token), wrong_amount).await,
        StatusCode::OK
    );
    let attached = json!({"id":"evt_subscription","event":"SUBSCRIPTION_CREATED", "dateCreated":format!("{today} 10:00:00"),
        "subscription":{"id":"sub_test","checkoutSession":id.to_string(),"status":"ACTIVE"}});
    assert_eq!(webhook(&app, Some(&token), attached).await, StatusCode::OK);
    assert_eq!(
        billing::paid_through(&pool, "sandbox", &est, today)
            .await
            .unwrap(),
        None
    );

    let confirmed = payment(
        "evt_confirmed",
        "PAYMENT_CONFIRMED",
        "10:01:00",
        json!(29.90),
    );
    let (x, y) = tokio::join!(
        webhook(&app, Some(&token), confirmed.clone()),
        webhook(&app, Some(&token), confirmed)
    );
    assert_eq!((x, y), (StatusCode::OK, StatusCode::OK));
    let recorded: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM billing_webhook_events WHERE event_id = 'evt_confirmed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recorded, 1);
    assert!(
        billing::paid_through(&pool, "sandbox", &est, today)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        billing::paid_through(&pool, "production", &est, today)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        billing::paid_through(&pool, "sandbox", &other_est, today)
            .await
            .unwrap(),
        None
    );
    let res = post_form(
        &app,
        &bob,
        "/dashboard/billing/cancel",
        format!("_csrf={}&subscription=sub_test&confirm=yes", bob.csrf_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let res = post_form(&app, &alice, "/dashboard/publish", publish(&alice)).await;
    assert_eq!(res.headers()[header::LOCATION], "/dashboard");

    let ended = json!({"id":"evt_deleted", "event":"SUBSCRIPTION_DELETED", "dateCreated":format!("{today} 10:02:00"), "subscription":{"id":"sub_test"}});
    assert_eq!(webhook(&app, Some(&token), ended).await, StatusCode::OK);
    assert!(
        billing::paid_through(&pool, "sandbox", &est, today)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        !billing::reserve_checkout(
            &pool,
            "sandbox",
            &est,
            &Uuid::new_v4().to_string(),
            today,
            BillingPlan::Monthly
        )
        .await
        .unwrap()
    );

    let refunded = payment("evt_refunded", "PAYMENT_REFUNDED", "10:03:00", json!(29.90));
    let delayed = payment("evt_delayed", "PAYMENT_RECEIVED", "10:02:00", json!(29.90));
    assert_eq!(webhook(&app, Some(&token), refunded).await, StatusCode::OK);
    assert_eq!(webhook(&app, Some(&token), delayed).await, StatusCode::OK);
    let res = post_form(&app, &alice, "/dashboard/publish", publish(&alice)).await;
    assert_eq!(res.headers()[header::LOCATION], "/dashboard/billing");
    let public = app
        .oneshot(
            Request::get("/c/test-billing-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public.status(), StatusCode::OK);
}

#[tokio::test]
async fn discounted_charges_grant_only_the_reserved_period_and_price() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));
    let env = "sandbox";
    for (plan, amount, due, expires) in [
        (
            BillingPlan::Semiannual,
            json!(170.43),
            "2026-08-31",
            "2027-02-28",
        ),
        (
            BillingPlan::Annual,
            json!(322.92),
            "2024-02-29",
            "2025-02-28",
        ),
    ] {
        let slug = format!("test-{}", plan.key());
        let (_, est) = owner(&pool, &slug).await;
        let reference = Uuid::new_v4().to_string();
        let checkout = Uuid::new_v4().to_string();
        let subscription = format!("sub_{}", plan.key());
        let payment = format!("pay_{}", plan.key());
        let today = due.parse().unwrap();
        assert!(
            billing::reserve_checkout(&pool, env, &est, &reference, today, plan)
                .await
                .unwrap()
        );
        billing::finish_checkout(&pool, env, &est, &reference, Some(&checkout))
            .await
            .unwrap();

        let payment_event = |key: &str, kind: &str, time: &str, value: &Value| {
            json!({
                "id": format!("{slug}-{key}"), "event": kind, "dateCreated": format!("{due} {time}"),
                "payment": {"id": payment, "subscription": subscription, "dueDate": due, "value": value}
            })
        };
        let wrong = payment_event("wrong", "PAYMENT_CONFIRMED", "10:00:00", &json!(29.90));
        assert_eq!(webhook(&app, Some(&token), wrong).await, StatusCode::OK);
        assert!(
            billing::paid_through(&pool, env, &est, today)
                .await
                .unwrap()
                .is_none()
        );
        let exact = payment_event("exact", "PAYMENT_CONFIRMED", "10:01:00", &amount);
        assert_eq!(webhook(&app, Some(&token), exact).await, StatusCode::OK);
        assert!(
            billing::paid_through(&pool, env, &est, today)
                .await
                .unwrap()
                .is_none()
        );
        let link = json!({"id": format!("{slug}-link"), "event": "SUBSCRIPTION_CREATED",
        "dateCreated": format!("{due} 10:02:00"), "subscription": {
            "id": subscription, "checkoutSession": checkout, "status": "ACTIVE"
        }});
        assert_eq!(webhook(&app, Some(&token), link).await, StatusCode::OK);
        let coverage = billing::paid_coverage(&pool, env, &est, today)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(coverage.plan_months, plan.months());
        assert_eq!(coverage.price_centavos, plan.price());
        assert_eq!(
            coverage.paid_through,
            expires.parse::<chrono::NaiveDate>().unwrap()
        );
        assert!(
            billing::paid_through(&pool, env, &est, coverage.paid_through)
                .await
                .unwrap()
                .is_none()
        );
        let reversal = payment_event("refund", "PAYMENT_REFUNDED", "10:01:00", &amount);
        assert_eq!(webhook(&app, Some(&token), reversal).await, StatusCode::OK);
        assert!(
            billing::paid_through(&pool, env, &est, today)
                .await
                .unwrap()
                .is_none()
        );
    }
}
