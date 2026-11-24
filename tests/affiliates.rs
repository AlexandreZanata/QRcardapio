use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::{Duration, NaiveDate};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::affiliates::{
    self as aff_service, AdminEnrollAffiliateInput, AdminUpdateAffiliateInput,
    SelfActivateAffiliateInput, UpdateProgramSettingsInput,
};
use qrcardapio::application::auth::{AuthSuccess, register_owner};
use qrcardapio::application::establishments::{CreateEstablishmentInput, create_establishment};
use qrcardapio::config::{AsaasConfig, AsaasEnvironment, Config, DbSynchronous, LogFormat};
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    affiliate_ledger, affiliates as aff_queries, billing,
};
use qrcardapio::infrastructure::security::PasswordService;

fn webhook_token() -> String {
    "a".repeat(40)
}

fn test_config(dir: &TempDir, token: &str) -> Config {
    Config {
        mcp_enabled: false,
        google: None,
        asaas: Some(
            AsaasConfig::new(AsaasEnvironment::Sandbox, "test-api-credential", token).unwrap(),
        ),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "https://qrcardapio.com.br".to_owned(),
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

async fn create_account(pool: &SqlitePool, email: &str) -> AuthSuccess {
    register_owner(
        pool,
        &PasswordService::new(),
        email,
        "TestPassword123",
        "TestPassword123",
        Locale::PtBr,
    )
    .await
    .unwrap()
}

async fn promote_admin(pool: &SqlitePool, account_id: &str) {
    sqlx::query("UPDATE accounts SET role = 'admin' WHERE id = ?")
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn create_est(pool: &SqlitePool, account_id: &str, slug: &str) -> String {
    create_establishment(
        pool,
        account_id,
        CreateEstablishmentInput {
            name: &format!("Loja {slug}"),
            slug,
            description: "",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap()
    .id
}

async fn open_checkout(pool: &SqlitePool, env: &str, est_id: &str, plan: BillingPlan) -> Uuid {
    let reference = Uuid::now_v7().to_string();
    let checkout_id = Uuid::now_v7();
    assert!(
        billing::reserve_checkout(
            pool,
            env,
            est_id,
            &reference,
            billing_now().date_naive(),
            plan
        )
        .await
        .unwrap()
    );
    billing::finish_checkout(
        pool,
        env,
        est_id,
        &reference,
        Some(&checkout_id.to_string()),
    )
    .await
    .unwrap();
    checkout_id
}

async fn send_webhook(app: &Router, token: &str, payload: Value) -> StatusCode {
    app.clone()
        .oneshot(
            Request::post("/webhooks/asaas")
                .header(header::CONTENT_TYPE, "application/json")
                .header("asaas-access-token", token)
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[allow(clippy::too_many_arguments)]
fn payment_event(
    event_id: &str,
    event_name: &str,
    payment_id: &str,
    subscription_id: &str,
    checkout_id: Option<Uuid>,
    due_date: NaiveDate,
    value_reais: &str,
    at: &str,
) -> Value {
    let mut pay = json!({
        "id": payment_id,
        "subscription": subscription_id,
        "dueDate": due_date.to_string(),
        "originalDueDate": due_date.to_string(),
        "value": value_reais.parse::<serde_json::Number>().unwrap()
    });
    if let Some(chk) = checkout_id {
        pay["checkoutSession"] = json!(chk.to_string());
    }
    json!({
        "id": event_id,
        "event": event_name,
        "dateCreated": at,
        "payment": pay
    })
}

fn auth_req(uri: &str, login: &AuthSuccess) -> axum::http::request::Builder {
    Request::builder().uri(uri).header(
        header::COOKIE,
        format!("qr_session={}", login.raw_session_token),
    )
}

async fn post_form(
    app: &Router,
    login: &AuthSuccess,
    uri: &str,
    body: String,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            auth_req(uri, login)
                .method("POST")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn body_text(res: axum::response::Response) -> String {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn extract_cookie(res: &axum::response::Response, name: &str) -> Option<String> {
    res.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|raw| {
            let pair = raw.split(';').next()?;
            let (k, v) = pair.split_once('=')?;
            (k.trim() == name).then(|| v.to_owned())
        })
}

#[tokio::test]
async fn self_activation_referral_cookie_quota_snapshot_and_payout_lifecycle() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let admin = create_account(&pool, "admin@example.test").await;
    promote_admin(&pool, &admin.account.id).await;
    // Pure affiliate account without any establishment!
    let partner = create_account(&pool, "parceiro@example.test").await;

    // 1. Self-activate affiliate via HTTP form.
    let res = post_form(
        &app,
        &partner,
        "/dashboard/affiliate/activate",
        format!(
            "_csrf={}&code=joao-parceiro&pix_key=joao@pix.test",
            partner.csrf_token
        ),
    )
    .await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let aff = aff_queries::find_affiliate_by_account(&pool, &partner.account.id)
        .await
        .unwrap()
        .expect("affiliate created");
    assert_eq!(aff.code, "joao-parceiro");
    assert_eq!(aff.commission_value, 500);
    assert_eq!(aff.hold_days, 15);

    // 2. Visitor clicks /r/joao-parceiro -> sets qr_ref cookie, no-store, counts click once.
    let click1 = app
        .clone()
        .oneshot(
            Request::get("/r/joao-parceiro")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(click1.status(), StatusCode::SEE_OTHER);
    assert_eq!(click1.headers().get(header::LOCATION).unwrap(), "/");
    assert_eq!(
        click1.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let ref_cookie = extract_cookie(&click1, "qr_ref").expect("qr_ref cookie set");
    assert_eq!(ref_cookie, "joao-parceiro");

    // Second click carrying the same cookie must NOT inflate click_count.
    let click2 = app
        .clone()
        .oneshot(
            Request::get("/r/joao-parceiro")
                .header(header::COOKIE, "qr_ref=joao-parceiro")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(click2.status(), StatusCode::SEE_OTHER);
    let aff_after_clicks = aff_queries::find_affiliate_by_id(&pool, &aff.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(aff_after_clicks.click_count, 1);

    // 3. Referred visitor registers via HTTP carrying qr_ref cookie -> attributed + cookie cleared.
    let reg = app
        .clone()
        .oneshot(
            Request::post("/register")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, "qr_ref=joao-parceiro")
                .body(Body::from(
                    "email=cliente1@padaria.test&password=TestPassword123&password_confirm=TestPassword123",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::SEE_OTHER);
    assert_eq!(extract_cookie(&reg, "qr_ref").as_deref(), Some(""));

    let customer = qrcardapio::infrastructure::database::queries::find_account_by_email(
        &pool,
        "cliente1@padaria.test",
    )
    .await
    .unwrap()
    .unwrap();
    let est_id = create_est(&pool, &customer.id, "padaria-indicada").await;

    // 4. Admin customizes affiliate monthly quota to R$ 8,50/month before payment.
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "joao-parceiro",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "8,50",
            hold_days: 15,
            pix_key: "joao@pix.test",
            notes: "Parceiro VIP",
        },
    )
    .await
    .unwrap();

    let today = billing_now().date_naive();
    let chk = open_checkout(&pool, "sandbox", &est_id, BillingPlan::Monthly).await;

    // 5. Asaas confirms first monthly payment (29.90) + sends duplicate PAYMENT_RECEIVED.
    let ev1 = payment_event(
        "evt_m1_conf",
        "PAYMENT_CONFIRMED",
        "pay_m1",
        "sub_m1",
        Some(chk),
        today,
        "29.90",
        "2026-11-24 10:00:00",
    );
    assert_eq!(send_webhook(&app, &token, ev1).await, StatusCode::OK);
    let ev1_dup = payment_event(
        "evt_m1_recv",
        "PAYMENT_RECEIVED",
        "pay_m1",
        "sub_m1",
        Some(chk),
        today,
        "29.90",
        "2026-12-24 10:00:00",
    );
    assert_eq!(send_webhook(&app, &token, ev1_dup).await, StatusCode::OK);

    let bal_day0 = affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
        .await
        .unwrap();
    assert_eq!(bal_day0.pending_centavos, 850);
    assert_eq!(bal_day0.available_centavos, 0);

    // 6. Admin changes quota to 20% -> old payment snapshot stays 850, renewal accrues 598 (20% of 2990).
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "joao-parceiro",
            status: "active",
            commission_type: "percent_bps",
            commission_value: "20",
            hold_days: 15,
            pix_key: "joao@pix.test",
            notes: "Passou para 20%",
        },
    )
    .await
    .unwrap();

    let next_due = today + Duration::days(30);
    let ev2 = payment_event(
        "evt_m2_conf",
        "PAYMENT_CONFIRMED",
        "pay_m2",
        "sub_m1",
        None,
        next_due,
        "29.90",
        "2026-12-24 11:00:00",
    );
    assert_eq!(send_webhook(&app, &token, ev2).await, StatusCode::OK);

    // Before hold expires, payout must fail.
    assert!(
        aff_service::settle_payout(
            &pool,
            &aff.id,
            "sandbox",
            &admin.account.id,
            "PIX-001",
            "",
            today + Duration::days(14),
        )
        .await
        .is_err()
    );

    // After payment #1 hold expires (today + 15d), only 850 is payable; payment #2 (598) stays pending.
    let pay_day = today + Duration::days(15);
    let bal_day15 = affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", pay_day)
        .await
        .unwrap();
    assert_eq!(bal_day15.available_centavos, 850);
    assert_eq!(bal_day15.pending_centavos, 598);

    let settled = aff_service::settle_payout(
        &pool,
        &aff.id,
        "sandbox",
        &admin.account.id,
        "E2E-PIX-123",
        "Lote nov",
        pay_day,
    )
    .await
    .unwrap();
    assert_eq!(settled.total_centavos, 850);
    assert_eq!(settled.entries_count, 1);

    // Duplicate immediate payout must be rejected.
    assert!(
        aff_service::settle_payout(
            &pool,
            &aff.id,
            "sandbox",
            &admin.account.id,
            "E2E-PIX-DUP",
            "",
            pay_day,
        )
        .await
        .is_err()
    );

    let bal_after = affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", pay_day)
        .await
        .unwrap();
    assert_eq!(bal_after.paid_centavos, 850);
    assert_eq!(bal_after.available_centavos, 0);
    assert_eq!(bal_after.pending_centavos, 598);
}

#[tokio::test]
async fn semiannual_and_annual_vest_month_by_month_and_cap_to_payment() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let _partner = create_account(&pool, "multi@example.test").await;
    let aff = aff_service::admin_enroll_affiliate(
        &pool,
        AdminEnrollAffiliateInput {
            email: "multi@example.test",
            code: "multi-cycle",
            status: "active",
            commission_type: "percent_bps",
            commission_value: "20",
            hold_days: 15,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();

    let today = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();

    // Semiannual customer (170.43 * 20% = 3409 centavos across 6 monthly installments)
    let cust_semi = create_account(&pool, "semi@example.test").await;
    assert!(
        aff_service::attribute_new_account(
            &pool,
            Some("multi-cycle"),
            &cust_semi.account.id,
            &cust_semi.account.email,
        )
        .await
        .unwrap()
    );
    let est_semi = create_est(&pool, &cust_semi.account.id, "loja-semi").await;
    let chk_semi = open_checkout(&pool, "sandbox", &est_semi, BillingPlan::Semiannual).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_semi",
                "PAYMENT_CONFIRMED",
                "pay_semi",
                "sub_semi",
                Some(chk_semi),
                today,
                "170.43",
                "2026-01-31 12:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );

    let ledger =
        affiliate_ledger::list_commissions_for_affiliate(&pool, &aff.id, "sandbox", today, 20)
            .await
            .unwrap();
    assert_eq!(ledger.len(), 6);
    assert_eq!(
        ledger.iter().map(|r| r.commission_centavos).sum::<i64>(),
        3409
    );

    // Month 1 available on 2026-02-15 (569), Month 2 on 2026-03-15 (568 -> cumulative 1137).
    let b1 = affiliate_ledger::affiliate_balances(
        &pool,
        &aff.id,
        "sandbox",
        NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(b1.available_centavos, 569);
    assert_eq!(b1.pending_centavos, 2840);

    let b2 = affiliate_ledger::affiliate_balances(
        &pool,
        &aff.id,
        "sandbox",
        NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(b2.available_centavos, 1137);

    // Annual customer with affiliate fixed quota at max (29,90/month -> 2990*12=35880 > 32292 -> capped at 32292)
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "multi-cycle",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "29,90",
            hold_days: 0,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();
    let cust_ann = create_account(&pool, "anual@example.test").await;
    assert!(
        aff_service::attribute_new_account(
            &pool,
            Some("multi-cycle"),
            &cust_ann.account.id,
            &cust_ann.account.email,
        )
        .await
        .unwrap()
    );
    let est_ann = create_est(&pool, &cust_ann.account.id, "loja-anual").await;
    let chk_ann = open_checkout(&pool, "sandbox", &est_ann, BillingPlan::Annual).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_ann",
                "PAYMENT_CONFIRMED",
                "pay_ann",
                "sub_ann",
                Some(chk_ann),
                today,
                "322.92",
                "2026-01-31 12:05:00",
            ),
        )
        .await,
        StatusCode::OK
    );

    let ann_sum: i64 = sqlx::query_scalar(
        "SELECT SUM(commission_centavos) FROM affiliate_commissions
         WHERE payment_id = 'pay_ann' AND entry_kind = 'commission'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ann_sum, 32_292);
}

#[tokio::test]
async fn refund_and_chargeback_void_unpaid_installments_and_net_paid_clawbacks() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let admin = create_account(&pool, "admin-cb@example.test").await;
    promote_admin(&pool, &admin.account.id).await;
    let _partner = create_account(&pool, "cb-partner@example.test").await;
    let aff = aff_service::admin_enroll_affiliate(
        &pool,
        AdminEnrollAffiliateInput {
            email: "cb-partner@example.test",
            code: "cb-partner",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "10,00",
            hold_days: 15,
            pix_key: "cb@pix.test",
            notes: "",
        },
    )
    .await
    .unwrap();

    let due = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
    let cust1 = create_account(&pool, "cust1-cb@example.test").await;
    aff_service::attribute_new_account(
        &pool,
        Some("cb-partner"),
        &cust1.account.id,
        &cust1.account.email,
    )
    .await
    .unwrap();
    let est1 = create_est(&pool, &cust1.account.id, "loja-cb-1").await;
    let chk1 = open_checkout(&pool, "sandbox", &est1, BillingPlan::Semiannual).await;

    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_cb_paid",
                "PAYMENT_CONFIRMED",
                "pay_cb_1",
                "sub_cb_1",
                Some(chk1),
                due,
                "170.43",
                "2026-05-01 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );

    // Month 1 vests on 2026-05-16 (1000 centavos) -> settled in Payout #1; months 2..=6 stay pending.
    let day_m1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
    let p1 = aff_service::settle_payout(
        &pool,
        &aff.id,
        "sandbox",
        &admin.account.id,
        "PIX-1",
        "",
        day_m1,
    )
    .await
    .unwrap();
    assert_eq!(p1.total_centavos, 1000);

    // Customer files chargeback on 2026-05-20 -> months 2..=6 voided, month 1 gets -1000 clawback.
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_cb_req",
                "PAYMENT_CHARGEBACK_REQUESTED",
                "pay_cb_1",
                "sub_cb_1",
                None,
                due,
                "170.43",
                "2026-05-20 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    // Duplicate refund event must NOT create a second clawback!
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_cb_ref",
                "PAYMENT_REFUNDED",
                "pay_cb_1",
                "sub_cb_1",
                None,
                due,
                "170.43",
                "2026-05-21 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    // Stale PAYMENT_CONFIRMED (older timestamp) must be ignored and NOT re-credit!
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_cb_stale",
                "PAYMENT_CONFIRMED",
                "pay_cb_1",
                "sub_cb_1",
                None,
                due,
                "170.43",
                "2026-05-02 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );

    let after_cb = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
    let bal = affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", after_cb)
        .await
        .unwrap();
    assert_eq!(bal.pending_centavos, 0);
    assert_eq!(bal.available_centavos, -1000);
    assert_eq!(bal.voided_centavos, 5000);
    assert_eq!(bal.paid_centavos, 1000);

    // Negative balance cannot be paid out.
    assert!(
        aff_service::settle_payout(
            &pool,
            &aff.id,
            "sandbox",
            &admin.account.id,
            "PIX-BAD",
            "",
            after_cb
        )
        .await
        .is_err()
    );

    // Second customer subscribes with R$ 25,00 monthly quota -> vests +2500 -> nets to +1500!
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "cb-partner",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "25,00",
            hold_days: 0,
            pix_key: "cb@pix.test",
            notes: "",
        },
    )
    .await
    .unwrap();
    let cust2 = create_account(&pool, "cust2-cb@example.test").await;
    aff_service::attribute_new_account(
        &pool,
        Some("cb-partner"),
        &cust2.account.id,
        &cust2.account.email,
    )
    .await
    .unwrap();
    let est2 = create_est(&pool, &cust2.account.id, "loja-cb-2").await;
    let chk2 = open_checkout(&pool, "sandbox", &est2, BillingPlan::Monthly).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_cb_c2",
                "PAYMENT_CONFIRMED",
                "pay_cb_2",
                "sub_cb_2",
                Some(chk2),
                after_cb,
                "29.90",
                "2026-12-01 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );

    let p2 = aff_service::settle_payout(
        &pool,
        &aff.id,
        "sandbox",
        &admin.account.id,
        "PIX-NET",
        "",
        after_cb,
    )
    .await
    .unwrap();
    assert_eq!(p2.total_centavos, 1500);
    assert_eq!(p2.entries_count, 2);

    let final_bal = affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", after_cb)
        .await
        .unwrap();
    assert_eq!(final_bal.available_centavos, 0);
    assert_eq!(final_bal.paid_centavos, 2500);
}

#[tokio::test]
async fn blocks_self_referral_enforces_status_lifecycle_and_prevents_day30_retroactivity() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let partner = create_account(&pool, "joao.silva@gmail.com").await;
    let aff = aff_service::self_activate_affiliate(
        &pool,
        &partner.account.id,
        &partner.account.email,
        SelfActivateAffiliateInput {
            desired_code: "joao-guard",
            pix_key: "",
        },
    )
    .await
    .unwrap();

    // 1. Self-referral by same account or Gmail dot/+alias is rejected at signup.
    assert!(
        !aff_service::attribute_new_account(
            &pool,
            Some("joao-guard"),
            &partner.account.id,
            &partner.account.email,
        )
        .await
        .unwrap()
    );
    let alias_acc = create_account(&pool, "joaosilva+promo@googlemail.com").await;
    assert!(
        !aff_service::attribute_new_account(
            &pool,
            Some("joao-guard"),
            &alias_acc.account.id,
            &alias_acc.account.email,
        )
        .await
        .unwrap()
    );

    // 2. If affiliate becomes a member of a referred establishment before payment -> auto-disqualified!
    let collusion = create_account(&pool, "laranja@outro.test").await;
    assert!(
        aff_service::attribute_new_account(
            &pool,
            Some("joao-guard"),
            &collusion.account.id,
            &collusion.account.email,
        )
        .await
        .unwrap()
    );
    let collusion_est = create_est(&pool, &collusion.account.id, "loja-conluio").await;
    sqlx::query(
        "INSERT INTO establishment_members (establishment_id, account_id, member_role, created_at)
         VALUES (?, ?, 'manager', datetime('now'))",
    )
    .bind(&collusion_est)
    .bind(&partner.account.id)
    .execute(&pool)
    .await
    .unwrap();

    let today = billing_now().date_naive();
    let chk_col = open_checkout(&pool, "sandbox", &collusion_est, BillingPlan::Monthly).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_col",
                "PAYMENT_CONFIRMED",
                "pay_col",
                "sub_col",
                Some(chk_col),
                today,
                "29.90",
                "2026-11-24 09:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    let col_bal =
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today + Duration::days(30))
            .await
            .unwrap();
    assert_eq!(col_bal.pending_centavos + col_bal.available_centavos, 0);

    // 3. Legitimate referral attributed while active.
    let legit = create_account(&pool, "cliente-real@restaurante.test").await;
    assert!(
        aff_service::attribute_new_account(
            &pool,
            Some("joao-guard"),
            &legit.account.id,
            &legit.account.email,
        )
        .await
        .unwrap()
    );
    let legit_est = create_est(&pool, &legit.account.id, "restaurante-real").await;
    let chk_legit = open_checkout(&pool, "sandbox", &legit_est, BillingPlan::Monthly).await;

    // 4. Pause affiliate: rejects new referrals, but existing referral STILL accrues!
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "joao-guard",
            status: "paused",
            commission_type: "fixed_monthly_centavos",
            commission_value: "5,00",
            hold_days: 0,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();
    let blocked_new = create_account(&pool, "novo-pausado@restaurante.test").await;
    assert!(
        !aff_service::attribute_new_account(
            &pool,
            Some("joao-guard"),
            &blocked_new.account.id,
            &blocked_new.account.email,
        )
        .await
        .unwrap()
    );
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_paused_ok",
                "PAYMENT_CONFIRMED",
                "pay_legit_1",
                "sub_legit",
                Some(chk_legit),
                today,
                "29.90",
                "2026-11-24 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        500
    );

    // 5. Revoke affiliate: Day-0 renewal accrues 0, and re-activating before Day-30 PAYMENT_RECEIVED
    //    must NOT retroactively grant commission for the Day-0 payment!
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "joao-guard",
            status: "revoked",
            commission_type: "fixed_monthly_centavos",
            commission_value: "5,00",
            hold_days: 0,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_rev_d0",
                "PAYMENT_CONFIRMED",
                "pay_legit_2",
                "sub_legit",
                None,
                today + Duration::days(30),
                "29.90",
                "2026-12-24 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    // Re-activate affiliate before Day-30 settlement webhook arrives.
    aff_service::admin_update_affiliate(
        &pool,
        &aff.id,
        AdminUpdateAffiliateInput {
            code: "joao-guard",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "5,00",
            hold_days: 0,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_rev_d30",
                "PAYMENT_RECEIVED",
                "pay_legit_2",
                "sub_legit",
                None,
                today + Duration::days(30),
                "29.90",
                "2027-01-23 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    // Still only 500 from pay_legit_1 (pay_legit_2 was evaluated on Day 0 while revoked!).
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today + Duration::days(60))
            .await
            .unwrap()
            .available_centavos,
        500
    );
}

#[tokio::test]
async fn out_of_order_webhooks_reconciliation_and_mismatched_payment_value() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let _partner = create_account(&pool, "ooo@example.test").await;
    let aff = aff_service::admin_enroll_affiliate(
        &pool,
        AdminEnrollAffiliateInput {
            email: "ooo@example.test",
            code: "ooo-ref",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "7,00",
            hold_days: 0,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();

    let today = billing_now().date_naive();

    // Case A: PAYMENT_CONFIRMED arrives BEFORE SUBSCRIPTION_CREATED links checkoutSession.
    let c1 = create_account(&pool, "ooo1@example.test").await;
    aff_service::attribute_new_account(&pool, Some("ooo-ref"), &c1.account.id, &c1.account.email)
        .await
        .unwrap();
    let est1 = create_est(&pool, &c1.account.id, "loja-ooo-1").await;
    let chk1 = open_checkout(&pool, "sandbox", &est1, BillingPlan::Monthly).await;

    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_ooo_pay_first",
                "PAYMENT_CONFIRMED",
                "pay_ooo_1",
                "sub_ooo_1",
                None,
                today,
                "29.90",
                "2026-11-24 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        0
    );

    assert_eq!(
        send_webhook(
            &app,
            &token,
            json!({
                "id": "evt_ooo_sub_later",
                "event": "SUBSCRIPTION_CREATED",
                "dateCreated": "2026-11-24 10:01:00",
                "subscription": {
                    "id": "sub_ooo_1",
                    "checkoutSession": chk1.to_string(),
                    "status": "ACTIVE"
                }
            }),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        700
    );

    // Case B: PAYMENT_CONFIRMED arrives unlinked -> manual reconcile_subscription links checkout.
    let c2 = create_account(&pool, "ooo2@example.test").await;
    aff_service::attribute_new_account(&pool, Some("ooo-ref"), &c2.account.id, &c2.account.email)
        .await
        .unwrap();
    let est2 = create_est(&pool, &c2.account.id, "loja-ooo-2").await;
    let chk2 = open_checkout(&pool, "sandbox", &est2, BillingPlan::Monthly).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            json!({
                "id": "evt_chk2_paid",
                "event": "CHECKOUT_PAID",
                "dateCreated": "2026-11-24 10:02:00",
                "checkout": { "id": chk2.to_string() }
            }),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_ooo_pay2",
                "PAYMENT_CONFIRMED",
                "pay_ooo_2",
                "sub_ooo_2",
                None,
                today,
                "29.90",
                "2026-11-24 10:03:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    billing::reconcile_subscription(&pool, "sandbox", &est2, &chk2.to_string(), "sub_ooo_2")
        .await
        .unwrap();
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        1400
    );

    // Case C: Partial/mismatched amount (10.00 vs 29.90) must accrue ZERO commission.
    let c3 = create_account(&pool, "ooo3@example.test").await;
    aff_service::attribute_new_account(&pool, Some("ooo-ref"), &c3.account.id, &c3.account.email)
        .await
        .unwrap();
    let est3 = create_est(&pool, &c3.account.id, "loja-ooo-3").await;
    let chk3 = open_checkout(&pool, "sandbox", &est3, BillingPlan::Monthly).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_ooo_mismatch",
                "PAYMENT_CONFIRMED",
                "pay_ooo_3",
                "sub_ooo_3",
                Some(chk3),
                today,
                "10.00",
                "2026-11-24 10:04:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        1400
    );
}

#[tokio::test]
async fn rbac_csrf_privacy_self_signup_toggle_open_redirect_env_isolation_and_robots() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let admin = create_account(&pool, "admin-sec@example.test").await;
    promote_admin(&pool, &admin.account.id).await;
    let owner = create_account(&pool, "owner-sec@example.test").await;
    let referred = create_account(&pool, "segredo.cliente@empresa.test").await;

    // 1. Regular owner cannot access admin affiliate routes (403).
    let forbidden_get = app
        .clone()
        .oneshot(
            auth_req("/admin/affiliates", &owner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden_get.status(), StatusCode::FORBIDDEN);

    // 2. Missing/invalid CSRF is rejected with 403.
    let bad_csrf = post_form(
        &app,
        &owner,
        "/dashboard/affiliate/activate",
        "_csrf=wrong&code=meu-cod".to_owned(),
    )
    .await;
    assert_eq!(bad_csrf.status(), StatusCode::FORBIDDEN);

    // 3. Admin closes self-signup -> owner self-activation fails, but admin enroll succeeds.
    aff_service::update_program_settings(
        &pool,
        UpdateProgramSettingsInput {
            self_signup_enabled: false,
            default_commission_type: "fixed_monthly_centavos",
            default_commission_value: "6,00",
            default_hold_days: 15,
            cookie_ttl_days: 30,
        },
    )
    .await
    .unwrap();
    assert!(
        aff_service::self_activate_affiliate(
            &pool,
            &owner.account.id,
            &owner.account.email,
            SelfActivateAffiliateInput {
                desired_code: "bloqueado",
                pix_key: "",
            },
        )
        .await
        .is_err()
    );

    let aff = aff_service::admin_enroll_affiliate(
        &pool,
        AdminEnrollAffiliateInput {
            email: "owner-sec@example.test",
            code: "sec-code",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "6,00",
            hold_days: 0,
            pix_key: "sec@pix.test",
            notes: "",
        },
    )
    .await
    .unwrap();

    aff_service::attribute_new_account(
        &pool,
        Some("sec-code"),
        &referred.account.id,
        &referred.account.email,
    )
    .await
    .unwrap();

    // 4. Partner portal masks customer email (LGPD), while admin detail shows full email.
    let portal_res = app
        .clone()
        .oneshot(
            auth_req("/dashboard/affiliate", &owner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(portal_res.status(), StatusCode::OK);
    let portal_html = body_text(portal_res).await;
    assert!(portal_html.contains("se***@empresa.test"));
    assert!(!portal_html.contains("segredo.cliente@empresa.test"));

    let admin_detail_res = app
        .clone()
        .oneshot(
            auth_req(&format!("/admin/affiliates/{}", aff.id), &admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_detail_res.status(), StatusCode::OK);
    let admin_html = body_text(admin_detail_res).await;
    assert!(admin_html.contains("segredo.cliente@empresa.test"));

    // 5. Open-redirect protection on /r/{code} and canonical clean redirect on /?ref={code}.
    let open_redir = app
        .clone()
        .oneshot(
            Request::get("/r/sec-code?plan=https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(open_redir.status(), StatusCode::SEE_OTHER);
    assert_eq!(open_redir.headers().get(header::LOCATION).unwrap(), "/");

    let landing_ref = app
        .clone()
        .oneshot(Request::get("/?ref=sec-code").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(landing_ref.status(), StatusCode::SEE_OTHER);
    assert_eq!(landing_ref.headers().get(header::LOCATION).unwrap(), "/");
    assert_eq!(
        landing_ref.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(
        extract_cookie(&landing_ref, "qr_ref").as_deref(),
        Some("sec-code")
    );

    // 6. Environment isolation: sandbox commissions never appear in production balances.
    let est = create_est(&pool, &referred.account.id, "loja-env-iso").await;
    let chk = open_checkout(&pool, "sandbox", &est, BillingPlan::Monthly).await;
    let today = billing_now().date_naive();
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_iso_1",
                "PAYMENT_CONFIRMED",
                "pay_iso_1",
                "sub_iso_1",
                Some(chk),
                today,
                "29.90",
                "2026-11-24 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        600
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "production", today)
            .await
            .unwrap()
            .available_centavos,
        0
    );

    // 7. robots.txt disallows /r/ referral redirect routes.
    let robots = app
        .clone()
        .oneshot(Request::get("/robots.txt").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(robots.status(), StatusCode::OK);
    let robots_txt = body_text(robots).await;
    assert!(robots_txt.contains("Disallow: /r/"));
}

#[tokio::test]
async fn admin_http_payout_confirmation_disqualification_and_dispute_reversal() {
    let dir = TempDir::new().unwrap();
    let token = webhook_token();
    let config = test_config(&dir, &token);
    let pool = init_pool(&config).await.unwrap();
    let app = build_router(AppState::new(config, pool.clone()));

    let admin = create_account(&pool, "admin-http@example.test").await;
    promote_admin(&pool, &admin.account.id).await;
    let partner = create_account(&pool, "partner-http@example.test").await;

    let aff = aff_service::admin_enroll_affiliate(
        &pool,
        AdminEnrollAffiliateInput {
            email: "partner-http@example.test",
            code: "http-flow",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "9,00",
            hold_days: 0,
            pix_key: "chave-pix@http.test",
            notes: "",
        },
    )
    .await
    .unwrap();

    let today = billing_now().date_naive();

    // Customer 1: payment -> chargeback dispute (voided) -> newer PAYMENT_RECEIVED (restored!)
    let c1 = create_account(&pool, "disp1@example.test").await;
    aff_service::attribute_new_account(&pool, Some("http-flow"), &c1.account.id, &c1.account.email)
        .await
        .unwrap();
    let est1 = create_est(&pool, &c1.account.id, "loja-disp-1").await;
    let chk1 = open_checkout(&pool, "sandbox", &est1, BillingPlan::Monthly).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_d_conf",
                "PAYMENT_CONFIRMED",
                "pay_disp_1",
                "sub_disp_1",
                Some(chk1),
                today,
                "29.90",
                "2026-11-24 10:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_d_dispute",
                "PAYMENT_CHARGEBACK_DISPUTE",
                "pay_disp_1",
                "sub_disp_1",
                None,
                today,
                "29.90",
                "2026-11-24 11:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        0
    );
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_d_won",
                "PAYMENT_RECEIVED",
                "pay_disp_1",
                "sub_disp_1",
                None,
                today,
                "29.90",
                "2026-11-24 12:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        900
    );

    // Admin payout without confirm=yes must NOT settle balance.
    let no_confirm = post_form(
        &app,
        &admin,
        &format!("/admin/affiliates/{}/payout", aff.id),
        format!("_csrf={}&env=sandbox&reference=REF-NO", admin.csrf_token),
    )
    .await;
    assert_eq!(no_confirm.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        900
    );

    // Admin payout WITH confirm=yes settles atomically.
    let confirmed = post_form(
        &app,
        &admin,
        &format!("/admin/affiliates/{}/payout", aff.id),
        format!(
            "_csrf={}&env=sandbox&reference=E2E-HTTP-99&confirm=yes",
            admin.csrf_token
        ),
    )
    .await;
    assert_eq!(confirmed.status(), StatusCode::SEE_OTHER);
    let bal_paid = affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
        .await
        .unwrap();
    assert_eq!(bal_paid.available_centavos, 0);
    assert_eq!(bal_paid.paid_centavos, 900);

    // Partner portal displays the recorded PIX reference.
    let portal = app
        .clone()
        .oneshot(
            auth_req("/dashboard/affiliate", &partner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(portal.status(), StatusCode::OK);
    let portal_html = body_text(portal).await;
    assert!(portal_html.contains("E2E-HTTP-99"));

    // Customer 2: unpaid commission gets voided when admin disqualifies referral via HTTP.
    let c2 = create_account(&pool, "fraud2@example.test").await;
    aff_service::attribute_new_account(&pool, Some("http-flow"), &c2.account.id, &c2.account.email)
        .await
        .unwrap();
    let est2 = create_est(&pool, &c2.account.id, "loja-fraud-2").await;
    let chk2 = open_checkout(&pool, "sandbox", &est2, BillingPlan::Monthly).await;
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_f2_1",
                "PAYMENT_CONFIRMED",
                "pay_f2_1",
                "sub_f2",
                Some(chk2),
                today,
                "29.90",
                "2026-11-24 13:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        900
    );

    let refs = aff_queries::list_referrals_for_affiliate(&pool, &aff.id, 10)
        .await
        .unwrap();
    let fraud_ref = refs
        .iter()
        .find(|r| r.referred_email == "fraud2@example.test")
        .unwrap();

    let disq = post_form(
        &app,
        &admin,
        &format!(
            "/admin/affiliates/{}/referrals/{}/disqualify",
            aff.id, fraud_ref.id
        ),
        format!("_csrf={}&reason=fraud_review", admin.csrf_token),
    )
    .await;
    assert_eq!(disq.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today)
            .await
            .unwrap()
            .available_centavos,
        0
    );

    // Renewal payment from disqualified referral must accrue 0.
    assert_eq!(
        send_webhook(
            &app,
            &token,
            payment_event(
                "evt_f2_2",
                "PAYMENT_CONFIRMED",
                "pay_f2_2",
                "sub_f2",
                None,
                today + Duration::days(30),
                "29.90",
                "2026-12-24 13:00:00",
            ),
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        affiliate_ledger::affiliate_balances(&pool, &aff.id, "sandbox", today + Duration::days(60))
            .await
            .unwrap()
            .available_centavos,
        0
    );
}
