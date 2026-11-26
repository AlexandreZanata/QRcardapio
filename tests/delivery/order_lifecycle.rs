use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use qrcardapio::application::auth::register_owner;
use qrcardapio::application::commerce::{order_expiry::expire_due, worker::CONSUMERS};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::database::queries::delivery_coupons;
use qrcardapio::infrastructure::events::dispatcher::dispatch_batch;
use qrcardapio::infrastructure::security::PasswordService;

use crate::fixture::{
    ORIGIN, Shop, checkout, expect_json, get_public, grant_preview, post_json, preview_router,
    submit,
};
use crate::orders::{assert_private_response, order_count, place, quote, tracking_api};
use crate::support::{harness, post_form};

const ORDERS: &str = "/dashboard/delivery/orders";

#[tokio::test]
async fn a_coupon_cannot_be_overbooked_and_expiration_releases_it_for_another_order() {
    let h = harness(false).await;
    let app = preview_router(&h).await;
    let shop = Shop::create(&h, "delivery-coupon-expiry", 0).await;
    grant_preview(&h, &shop).await;
    assert_eq!(
        submit(
            &app,
            &shop.owner,
            "/dashboard/delivery/coupons",
            "code=REUSO&kind=amount_centavos&value=3%2C00&max_redemptions=1&allow_pickup=1&is_active=1",
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    shop.publish(&app).await;
    let input = shop.quote(shop.catalog_version(&h).await, None, Some("REUSO"));
    let priced = quote(&app, &shop, &input).await;
    assert_eq!(priced["coupon"]["status"], "applied");
    assert_eq!(priced["discount_centavos"], 300);
    let payload = checkout(&input, &priced, None);
    let orders_path = shop.api("orders");
    let first_key = Uuid::new_v4();
    let second_key = Uuid::new_v4();
    let (first, second) = tokio::join!(
        post_json(&app, &orders_path, &payload, ORIGIN, Some(first_key)),
        post_json(&app, &orders_path, &payload, ORIGIN, Some(second_key)),
    );
    let (winner_key, created, rejected) = if first.status() == StatusCode::CREATED {
        (first_key, first, second)
    } else {
        (second_key, second, first)
    };
    let receipt = expect_json(created, StatusCode::CREATED).await;
    let repriced = expect_json(rejected, StatusCode::CONFLICT).await;
    assert_eq!(repriced["code"], "commerce.checkout_repriced");
    assert_eq!(repriced["quote"]["coupon"]["status"], "invalid");
    assert_eq!(repriced["quote"]["discount_centavos"], 0);
    assert_eq!(order_count(&h, &shop).await, 1);
    assert_eq!(place(&app, &shop, &payload, winner_key).await, receipt);

    let expires = Utc::now() + Duration::minutes(20);
    assert_eq!(expire_due(&h.pool, expires).await.unwrap(), 1);
    dispatch_batch(&h.pool, CONSUMERS, expires).await.unwrap();
    let tracking = expect_json(
        get_public(&app, &tracking_api(&shop, &receipt), None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(tracking["state"], "rejected");
    assert_eq!(tracking["reason_code"], "acceptance_timeout");
    assert_eq!(tracking["payment"]["state"], "cancelled");
    let coupon = delivery_coupons::get_by_code_in(&h.pool, &shop.id, "REUSO")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(coupon.redemption_count, 0);
    let next = place(&app, &shop, &payload, Uuid::new_v4()).await;
    assert_ne!(receipt["order_id"], next["order_id"]);
    assert_eq!(order_count(&h, &shop).await, 2);
}

#[tokio::test]
async fn staff_updates_require_the_right_tenant_role_csrf_and_order_version() {
    let h = harness(false).await;
    let app = preview_router(&h).await;
    let shop = Shop::create(&h, "delivery-staff-alpha", 0).await;
    let other = Shop::create(&h, "delivery-staff-beta", 0).await;
    grant_preview(&h, &shop).await;
    grant_preview(&h, &other).await;
    shop.publish(&app).await;
    let waiter = register_owner(
        &h.pool,
        &PasswordService::new(),
        "delivery-waiter@example.test",
        "TestPassword123",
        "TestPassword123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO establishment_member_roles (establishment_id, account_id, member_role, created_at)
         VALUES (?, ?, 'waiter', datetime('now'))",
    )
    .bind(&shop.id)
    .bind(&waiter.account.id)
    .execute(&h.pool)
    .await
    .unwrap();

    let input = shop.quote(shop.catalog_version(&h).await, None, None);
    let priced = quote(&app, &shop, &input).await;
    let order = place(
        &app,
        &shop,
        &checkout(&input, &priced, None),
        Uuid::new_v4(),
    )
    .await;
    let (id, version): (String, i64) = sqlx::query_as(
        "SELECT id, version FROM orders WHERE establishment_id = ? AND public_id = ?",
    )
    .bind(&shop.id)
    .bind(order["order_id"].as_str().unwrap())
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let transition = format!("{ORDERS}/{id}/transition");
    let accept = format!("to=accepted&expected_version={version}&prep_minutes=15");
    assert_eq!(
        post_form(
            &app,
            &shop.owner,
            &transition,
            format!("_csrf=invalid&{accept}")
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        submit(&app, &other.owner, &transition, &accept)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        submit(&app, &waiter, &transition, &accept).await.status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        submit(&app, &waiter, &transition, &accept).await.status(),
        StatusCode::CONFLICT
    );
    let cancel = format!(
        "to=cancelled&expected_version={}&reason_code=other",
        version + 1
    );
    assert_eq!(
        submit(&app, &waiter, &transition, &cancel).await.status(),
        StatusCode::FORBIDDEN
    );
    let tracking = tracking_api(&shop, &order);
    assert_eq!(
        post_json(
            &app,
            &format!("{tracking}/cancel"),
            &json!({ "cancel_token": order["cancel_token"] }),
            ORIGIN,
            None,
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        submit(&app, &shop.owner, &transition, &cancel)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let result = expect_json(get_public(&app, &tracking, None).await, StatusCode::OK).await;
    assert_eq!(result["state"], "cancelled");
    assert_private_response(&result, &order);
}
