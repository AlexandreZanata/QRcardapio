//! Stripe and Banco Inter SaaS billing against loopback mocks of their APIs (no network).

#[path = "commercial/support.rs"]
mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get as route_get, post as route_post};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::billing_inter;
use qrcardapio::application::entitlements::{Enforcement, resolve_capability};
use qrcardapio::application::payments::BillingGateways;
use qrcardapio::config::{AsaasEnvironment, InterConfig, PaymentsConfig, StripeConfig};
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::domain::entitlements::{Capability, EntitlementStatus};
use qrcardapio::domain::payments::PaymentProvider;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::billing;
use qrcardapio::infrastructure::inter::InterClient;
use qrcardapio::infrastructure::stripe::{StripeClient, signature};
use support::*;

const STRIPE_SECRET: &str = "whsec_test_signing_secret_0001";
const INTER_TOKEN: &str = "interWebhookToken0123456789abcdefXYZ";

async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

async fn body(response: axum::response::Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

fn enforced() -> Enforcement {
    Enforcement::Enforced { environment: ENV }
}

// ---------------------------------------------------------------- Stripe

async fn stripe_mock() -> String {
    let router = Router::new()
        .route(
            "/v1/invoice_payments",
            route_get(|| async { axum::Json(json!({"data": [{"invoice": "in_paid1"}]})) }),
        )
        .route(
            "/v1/subscriptions/{id}",
            route_post(|Path(id): Path<String>| async move { axum::Json(json!({"id": id})) }),
        );
    serve(router).await
}

async fn stripe_event(app: &Router, event: Value, secret: &str) -> StatusCode {
    let raw = event.to_string();
    let header = signature::sign(
        raw.as_bytes(),
        secret.as_bytes(),
        chrono::Utc::now().timestamp(),
    );
    app.clone()
        .oneshot(
            Request::post("/webhooks/stripe")
                .header(header::CONTENT_TYPE, "application/json")
                .header("stripe-signature", header)
                .body(Body::from(raw))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn event_at(id: &str, kind: &str, object: Value, offset: i64) -> Value {
    json!({"id": id, "type": kind, "created": chrono::Utc::now().timestamp() + offset, "livemode": false,
           "data": {"object": object}})
}

fn event(id: &str, kind: &str, object: Value) -> Value {
    event_at(id, kind, object, 0)
}

#[tokio::test]
async fn stripe_card_subscription_follows_signed_events() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir, false);
    let pool = init_pool(&cfg).await.unwrap();
    let stripe_config = StripeConfig::new("sk_test_unit", STRIPE_SECRET).unwrap();
    let client = StripeClient::for_loopback_tests(stripe_config, &stripe_mock().await);
    let gateways = BillingGateways::default().with_stripe(client);
    let app = build_router(AppState::new(cfg, pool.clone()).with_billing(gateways));
    let (owner_login, est) = owner(&pool, "stripe-cafe").await;
    let today = billing_now().date_naive();

    // Reservation as created by the checkout flow (the Checkout API call itself is unit-tested).
    let reference = Uuid::now_v7().to_string();
    let quote = qrcardapio::domain::commercial::catalog().entry_quote(BillingPlan::Monthly);
    let target = billing::CheckoutTarget {
        provider: PaymentProvider::Stripe,
        method: qrcardapio::domain::payments::PaymentMethod::Card,
    };
    assert!(
        billing::reserve_contract_checkout(&pool, ENV, &est, &reference, today, &quote, target)
            .await
            .unwrap()
    );
    billing::finish_hosted_checkout(
        &pool,
        ENV,
        &est,
        &reference,
        Some("stripe:cs_test_a1"),
        Some("https://checkout.stripe.com/c/pay/cs_test_a1"),
    )
    .await
    .unwrap();
    let page =
        String::from_utf8(body(get(&app, &owner_login, "/dashboard/billing").await).await).unwrap();
    assert!(page.contains("https://checkout.stripe.com/c/pay/cs_test_a1"));

    // Authentication: wrong secret and tampered bodies are rejected.
    let completed = event(
        "evt_c1",
        "checkout.session.completed",
        json!({
            "id": "cs_test_a1", "mode": "subscription", "subscription": "sub_s1", "client_reference_id": reference,
        }),
    );
    assert_eq!(
        stripe_event(&app, completed.clone(), "whsec_wrong_secret_value").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        stripe_event(&app, completed.clone(), STRIPE_SECRET).await,
        StatusCode::OK
    );
    assert_eq!(
        stripe_event(&app, completed, STRIPE_SECRET).await,
        StatusCode::OK,
        "redelivery is a no-op"
    );
    let mut live = event("evt_live", "invoice.paid", json!({}));
    live["livemode"] = json!(true);
    assert_eq!(
        stripe_event(&app, live, STRIPE_SECRET).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let period_start = chrono::Utc::now().timestamp();
    let invoice = |amount: i64| {
        json!({
            "id": "in_paid1", "status": "paid", "currency": "brl", "amount_paid": amount, "amount_remaining": 0,
            "parent": {"type": "subscription_details", "subscription_details": {"subscription": "sub_s1"}},
            "lines": {"data": [{"parent": {"type": "subscription_item_details"},
                                 "period": {"start": period_start, "end": period_start + 2_678_400}}]}
        })
    };
    // A different amount never grants coverage.
    assert_eq!(
        stripe_event(
            &app,
            event("evt_i0", "invoice.paid", invoice(100)),
            STRIPE_SECRET
        )
        .await,
        StatusCode::OK
    );
    assert!(
        !resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );
    assert_eq!(
        stripe_event(
            &app,
            event_at("evt_i1", "invoice.paid", invoice(2990), 1),
            STRIPE_SECRET
        )
        .await,
        StatusCode::OK
    );
    let decision = resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
        .await
        .unwrap();
    assert_eq!(decision.status, EntitlementStatus::Active);
    let activated: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM domain_events WHERE event_type = 'SubscriptionActivated' AND establishment_id = ?")
        .bind(&est).fetch_one(&pool).await.unwrap();
    assert_eq!(activated, 1);

    // Dispute opened revokes; dispute won restores; a full refund revokes again.
    let dispute = |status: &str| json!({"id": "du_1", "status": status, "payment_intent": "pi_1"});
    assert_eq!(
        stripe_event(
            &app,
            event_at(
                "evt_d1",
                "charge.dispute.created",
                dispute("needs_response"),
                2
            ),
            STRIPE_SECRET
        )
        .await,
        StatusCode::OK
    );
    assert!(
        !resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );
    let mut won = event("evt_d2", "charge.dispute.closed", dispute("won"));
    won["created"] = json!(chrono::Utc::now().timestamp() + 5);
    assert_eq!(stripe_event(&app, won, STRIPE_SECRET).await, StatusCode::OK);
    assert!(
        resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );
    let mut refund = event(
        "evt_r1",
        "charge.refunded",
        json!({"refunded": true, "payment_intent": "pi_1"}),
    );
    refund["created"] = json!(chrono::Utc::now().timestamp() + 10);
    assert_eq!(
        stripe_event(&app, refund, STRIPE_SECRET).await,
        StatusCode::OK
    );
    assert!(
        !resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );

    // Cancelling renewal goes to Stripe (cancel_at_period_end) and frees the slot locally.
    let res = post_form(
        &app,
        &owner_login,
        "/dashboard/billing/cancel",
        format!(
            "_csrf={}&subscription=stripe:sub_s1&confirm=yes",
            owner_login.csrf_token
        ),
    )
    .await;
    assert_eq!(location(&res), "/dashboard/billing");
    assert!(
        billing::current_checkout(&pool, ENV, &est)
            .await
            .unwrap()
            .is_none()
    );
}

// ---------------------------------------------------------------- Inter

#[derive(Default)]
struct InterBank {
    charges: HashMap<String, Value>,
    created: Vec<Value>,
}

type Bank = Arc<Mutex<InterBank>>;

async fn inter_mock(bank: Bank) -> String {
    let router = Router::new()
        .route("/oauth/v2/token", route_post(|| async { axum::Json(json!({"access_token": "tok", "expires_in": 3600})) }))
        .route(
            "/cobranca/v3/cobrancas",
            route_post(|State(bank): State<Bank>, axum::Json(body): axum::Json<Value>| async move {
                let code = Uuid::now_v7().to_string();
                let mut bank = bank.lock().unwrap();
                bank.created.push(body.clone());
                bank.charges.insert(code.clone(), json!({
                    "cobranca": {"codigoSolicitacao": code, "seuNumero": body["seuNumero"], "situacao": "A_RECEBER",
                                 "dataVencimento": body["dataVencimento"]},
                    "boleto": {"linhaDigitavel": "07790001161234567890123456789012345678901234"},
                    "pix": {"pixCopiaECola": "00020101021226870014br.gov.bcb.pix2565qrcode-test"}
                }));
                axum::Json(json!({"codigoSolicitacao": code}))
            }),
        )
        .route(
            "/cobranca/v3/cobrancas/{code}",
            route_get(|State(bank): State<Bank>, Path(code): Path<String>| async move {
                match bank.lock().unwrap().charges.get(&code) {
                    Some(charge) => axum::Json(charge.clone()).into_response(),
                    None => StatusCode::NOT_FOUND.into_response(),
                }
            }),
        )
        .route(
            "/cobranca/v3/cobrancas/{code}/cancelar",
            route_post(|State(bank): State<Bank>, Path(code): Path<String>| async move {
                if let Some(charge) = bank.lock().unwrap().charges.get_mut(&code) {
                    charge["cobranca"]["situacao"] = json!("CANCELADO");
                }
                StatusCode::ACCEPTED
            }),
        )
        .route(
            "/cobranca/v3/cobrancas/{code}/pdf",
            route_get(|| async {
                use base64::Engine;
                axum::Json(json!({"pdf": base64::engine::general_purpose::STANDARD.encode(b"%PDF-1.4 mock")}))
            }),
        )
        .with_state(bank);
    serve(router).await
}

fn pay(bank: &Bank, code: &str, amount: &str) {
    let mut bank = bank.lock().unwrap();
    let charge = bank.charges.get_mut(code).unwrap();
    charge["cobranca"]["situacao"] = json!("RECEBIDO");
    charge["cobranca"]["valorTotalRecebido"] = json!(amount);
}

async fn inter_callback(app: &Router, token: &str, codes: &[&str]) -> StatusCode {
    let payload: Vec<Value> = codes
        .iter()
        .map(|c| json!({"codigoSolicitacao": c, "situacao": "RECEBIDO"}))
        .collect();
    app.clone()
        .oneshot(
            Request::post(format!("/webhooks/inter/{token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(Value::Array(payload).to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

const PAYER: &str = "document=11.222.333%2F0001-81&legal_name=Bistro+da+Praca+Ltda&email=financeiro%40bistro.test&postal_code=01310-100&street=Avenida+Paulista&number=1000&complement=&district=Bela+Vista&city=Sao+Paulo&state=SP";

#[tokio::test]
async fn inter_pix_boleto_subscription_issues_cycles_and_reconciles() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir, false);
    let pool = init_pool(&cfg).await.unwrap();
    let bank: Bank = Arc::default();
    let origin = inter_mock(bank.clone()).await;
    let inter_config = InterConfig::new(
        AsaasEnvironment::Sandbox,
        "client",
        "secret",
        Vec::new(),
        None,
        INTER_TOKEN,
    )
    .unwrap()
    .with_loopback_origin(&origin)
    .unwrap();
    let client = InterClient::new(inter_config).unwrap();
    let app = build_router(
        AppState::new(cfg, pool.clone())
            .with_billing(BillingGateways::default().with_inter(client.clone())),
    );
    let (login, est) = owner(&pool, "inter-bistro").await;
    let manager_login = manager(&pool, &est, "inter-manager@example.test").await;
    let today = billing_now().date_naive();

    // Payer data is required, validated and owner-only.
    let checkout = |login: &qrcardapio::application::auth::AuthSuccess| {
        format!("_csrf={}&plan=monthly&method=pix_boleto", login.csrf_token)
    };
    assert_eq!(
        post_form(
            &app,
            &login,
            "/dashboard/billing/checkout",
            checkout(&login)
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let bad = format!(
        "_csrf={}&{}",
        login.csrf_token,
        PAYER.replace("0001-81", "0001-80")
    );
    assert_eq!(
        post_form(&app, &login, "/dashboard/billing/payer", bad)
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let by_manager = format!("_csrf={}&{PAYER}", manager_login.csrf_token);
    assert_eq!(
        post_form(&app, &manager_login, "/dashboard/billing/payer", by_manager)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let res = post_form(
        &app,
        &login,
        "/dashboard/billing/payer",
        format!("_csrf={}&{PAYER}", login.csrf_token),
    )
    .await;
    assert_eq!(location(&res), "/dashboard/billing");
    let res = post_form(
        &app,
        &login,
        "/dashboard/billing/checkout",
        checkout(&login),
    )
    .await;
    assert_eq!(location(&res), "/dashboard/billing");

    // The first charge was issued with backend amount, payer and deterministic seuNumero.
    let (code, sent) = {
        let bank = bank.lock().unwrap();
        assert_eq!(bank.created.len(), 1);
        (
            bank.charges.keys().next().unwrap().clone(),
            bank.created[0].clone(),
        )
    };
    assert_eq!(sent["valorNominal"].to_string(), "29.90");
    assert_eq!(sent["pagador"]["cpfCnpj"], "11222333000181");
    assert_eq!(sent["seuNumero"].as_str().unwrap().len(), 15);
    // Double submission never issues a second charge.
    post_form(
        &app,
        &login,
        "/dashboard/billing/checkout",
        checkout(&login),
    )
    .await;
    assert_eq!(bank.lock().unwrap().created.len(), 1);

    // The page shows the charge after reconciliation fetched the Pix and boleto data.
    billing_inter::run_cycle(&pool, &client).await.unwrap();
    let page =
        String::from_utf8(body(get(&app, &login, "/dashboard/billing").await).await).unwrap();
    assert!(page.contains("qrcode-test") && page.contains("0779000116"));
    let svg = get(&app, &login, "/dashboard/billing/pix.svg").await;
    assert_eq!(
        svg.headers()[header::CONTENT_TYPE],
        "image/svg+xml; charset=utf-8"
    );
    let pdf = get(&app, &login, "/dashboard/billing/boleto.pdf").await;
    assert_eq!(pdf.headers()[header::CACHE_CONTROL], "no-store");
    assert!(body(pdf).await.starts_with(b"%PDF"));
    assert!(
        !resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );

    // Coverage starts at the due date; pin it to today (after 19:00 the first charge is due tomorrow).
    sqlx::query("UPDATE billing_inter_charges SET due_date = ? WHERE cycle_index = 0")
        .bind(today)
        .execute(&pool)
        .await
        .unwrap();
    // Callbacks: wrong token hidden, partial amount ignored, exact payment grants coverage.
    assert_eq!(
        inter_callback(&app, "wrong-token", &[&code]).await,
        StatusCode::NOT_FOUND
    );
    pay(&bank, &code, "10.00");
    assert_eq!(
        inter_callback(&app, INTER_TOKEN, &[&code]).await,
        StatusCode::OK
    );
    assert!(
        !resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );
    pay(&bank, &code, "29.90");
    let unknown = Uuid::now_v7().to_string();
    assert_eq!(
        inter_callback(&app, INTER_TOKEN, &[&code, &unknown]).await,
        StatusCode::OK
    );
    let decision = resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
        .await
        .unwrap();
    assert_eq!(decision.status, EntitlementStatus::Active);
    assert_eq!(decision.starts_on, Some(today));

    // Renewal: move the first due date back so the next cycle falls inside the issue window.
    sqlx::query("UPDATE billing_inter_charges SET due_date = ? WHERE cycle_index = 0")
        .bind(today - chrono::Duration::days(25))
        .execute(&pool)
        .await
        .unwrap();
    let (issued, _) = billing_inter::run_cycle(&pool, &client).await.unwrap();
    assert_eq!(issued, 1);
    assert_eq!(
        billing_inter::run_cycle(&pool, &client).await.unwrap().0,
        0,
        "one charge per cycle"
    );
    assert_eq!(bank.lock().unwrap().created.len(), 2);

    // Cancelling renewal cancels the pending charge at Inter; the paid period stays covered.
    let reference: String =
        sqlx::query_scalar("SELECT reference FROM billing_checkouts WHERE establishment_id = ?")
            .bind(&est)
            .fetch_one(&pool)
            .await
            .unwrap();
    let res = post_form(
        &app,
        &login,
        "/dashboard/billing/cancel",
        format!(
            "_csrf={}&subscription=inter:{reference}&confirm=yes",
            login.csrf_token
        ),
    )
    .await;
    assert_eq!(location(&res), "/dashboard/billing");
    let cancelled = bank
        .lock()
        .unwrap()
        .charges
        .values()
        .filter(|c| c["cobranca"]["situacao"] == "CANCELADO")
        .count();
    assert_eq!(cancelled, 1);
    assert!(
        resolve_capability(&pool, enforced(), &est, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );
}

#[tokio::test]
async fn unpaid_first_inter_charge_closes_the_checkout() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir, false);
    let pool = init_pool(&cfg).await.unwrap();
    let bank: Bank = Arc::default();
    let origin = inter_mock(bank.clone()).await;
    let client = InterClient::new(
        InterConfig::new(
            AsaasEnvironment::Sandbox,
            "client",
            "secret",
            Vec::new(),
            None,
            INTER_TOKEN,
        )
        .unwrap()
        .with_loopback_origin(&origin)
        .unwrap(),
    )
    .unwrap();
    let app = build_router(
        AppState::new(cfg, pool.clone())
            .with_billing(BillingGateways::default().with_inter(client.clone())),
    );
    let (login, est) = owner(&pool, "inter-expired").await;
    post_form(
        &app,
        &login,
        "/dashboard/billing/payer",
        format!("_csrf={}&{PAYER}", login.csrf_token),
    )
    .await;
    post_form(
        &app,
        &login,
        "/dashboard/billing/checkout",
        format!("_csrf={}&method=pix_boleto", login.csrf_token),
    )
    .await;
    let code = bank.lock().unwrap().charges.keys().next().unwrap().clone();
    bank.lock().unwrap().charges.get_mut(&code).unwrap()["cobranca"]["situacao"] =
        json!("EXPIRADO");
    assert_eq!(
        inter_callback(&app, INTER_TOKEN, &[&code]).await,
        StatusCode::OK
    );
    assert!(
        billing::current_checkout(&pool, ENV, &est)
            .await
            .unwrap()
            .is_none(),
        "slot freed for a new attempt"
    );
}

#[test]
fn provider_configuration_is_validated_together() {
    let asaas = qrcardapio::config::AsaasConfig::new(
        AsaasEnvironment::Sandbox,
        "asaas-key",
        &"t".repeat(40),
    )
    .unwrap();
    let stripe_live = StripeConfig::new("sk_live_x", STRIPE_SECRET).unwrap();
    let stripe_test = StripeConfig::new("sk_test_x", STRIPE_SECRET).unwrap();
    let mixed = PaymentsConfig {
        stripe: Some(stripe_live),
        inter: None,
        card_provider: Some(PaymentProvider::Stripe),
    };
    assert!(
        mixed
            .validate(Some(&asaas))
            .unwrap_err()
            .contains("same environment")
    );
    let ambiguous = PaymentsConfig {
        stripe: Some(stripe_test.clone()),
        inter: None,
        card_provider: None,
    };
    assert!(
        ambiguous
            .validate(Some(&asaas))
            .unwrap_err()
            .contains("BILLING_CARD_PROVIDER")
    );
    let routed = PaymentsConfig {
        stripe: Some(stripe_test),
        inter: None,
        card_provider: Some(PaymentProvider::Stripe),
    };
    assert!(routed.validate(Some(&asaas)).is_ok());
    assert!(StripeConfig::new("pk_test_x", STRIPE_SECRET).is_err());
    assert!(StripeConfig::new("sk_test_x", "not-a-secret").is_err());
    assert!(
        InterConfig::new(
            AsaasEnvironment::Sandbox,
            "c",
            "s",
            Vec::new(),
            None,
            "short"
        )
        .is_err()
    );
    let inter = InterConfig::new(
        AsaasEnvironment::Sandbox,
        "c",
        "s",
        Vec::new(),
        None,
        INTER_TOKEN,
    )
    .unwrap();
    assert!(inter.with_loopback_origin("https://evil.example").is_err());
}
