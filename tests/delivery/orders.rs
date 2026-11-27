use axum::Router;
use axum::http::{StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;

use qrcardapio::infrastructure::database::queries::product_variations;

use crate::fixture::{
    CUSTOMER, ORIGIN, PHONE, PRIVATE_NOTE, STREET, Shop, checkout, expect_json, get_public,
    grant_preview, post_json, preview_router,
};
use crate::order_fixtures::variation;
use crate::support::{Harness, harness};

pub(super) async fn quote(app: &Router, shop: &Shop, request: &Value) -> Value {
    expect_json(
        post_json(app, &shop.api("quotes"), request, ORIGIN, None).await,
        StatusCode::OK,
    )
    .await
}

pub(super) async fn place(app: &Router, shop: &Shop, request: &Value, key: Uuid) -> Value {
    expect_json(
        post_json(app, &shop.api("orders"), request, ORIGIN, Some(key)).await,
        StatusCode::CREATED,
    )
    .await
}

pub(super) async fn order_count(h: &Harness, shop: &Shop) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM orders WHERE establishment_id = ?")
        .bind(&shop.id)
        .fetch_one(&h.pool)
        .await
        .unwrap()
}

pub(super) fn tracking_api(shop: &Shop, receipt: &Value) -> String {
    let public_id = receipt["order_id"].as_str().unwrap();
    assert!(public_id.len() == 32 && public_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let path = shop.api(&format!("orders/{public_id}"));
    assert_eq!(receipt["tracking_path"], path);
    path
}

pub(super) fn assert_private_response(view: &Value, receipt: &Value) {
    for key in [
        "customer",
        "address",
        "phone",
        "note",
        "cancel_token",
        "cancel_token_hash",
    ] {
        assert!(view.get(key).is_none(), "unexpected field {key}: {view}");
    }
    let wire = view.to_string();
    for private in [
        CUSTOMER,
        PHONE,
        PHONE.trim_start_matches('+'),
        STREET,
        PRIVATE_NOTE,
        receipt["cancel_token"].as_str().unwrap(),
    ] {
        assert!(
            !wire.contains(private),
            "public tracking exposed private input: {view}"
        );
    }
}

#[tokio::test]
async fn republishing_across_free_delivery_threshold_requires_reconfirmation() {
    let h = harness(false).await;
    let app = preview_router(&h).await;
    let shop = Shop::create(&h, "delivery-shipping-threshold", 0).await;
    grant_preview(&h, &shop).await;
    let zone = shop.create_zone(&h, &app).await;
    sqlx::query(
        "UPDATE delivery_zones SET free_delivery_from_centavos = 5000
         WHERE id = ? AND establishment_id = ?",
    )
    .bind(&zone)
    .bind(&shop.id)
    .execute(&h.pool)
    .await
    .unwrap();
    shop.publish(&app).await;
    let original_version = shop.catalog_version(&h).await;
    let mut input = shop.quote(original_version, Some(&zone), None);
    let original = quote(&app, &shop, &input).await;
    assert_eq!(original["items_subtotal_centavos"], 4_500);
    assert_eq!(original["delivery_fee_centavos"], 500);
    let stale = checkout(&input, &original, Some(&zone));

    let mut updated = variation(shop.item.variation_id.as_deref().unwrap());
    updated.price_centavos = 2_250;
    product_variations::save(
        &h.pool,
        &shop.id,
        &shop.item.product_id,
        &updated,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
    shop.publish(&app).await;
    let published_version = shop.catalog_version(&h).await;
    assert!(published_version > original_version);

    let key = Uuid::new_v4();
    let repriced = expect_json(
        post_json(&app, &shop.api("orders"), &stale, ORIGIN, Some(key)).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(repriced["code"], "commerce.checkout_repriced");
    let fresh = &repriced["quote"];
    assert_eq!(fresh["catalog_version"], published_version);
    assert_eq!(fresh["items_subtotal_centavos"], 5_000);
    assert_eq!(fresh["delivery_fee_centavos"], 0);
    assert_eq!(fresh["total_centavos"], original["total_centavos"]);
    let writes: (i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM orders WHERE establishment_id = ?1),
           (SELECT count(*) FROM order_payments WHERE establishment_id = ?1),
           (SELECT count(*) FROM checkout_attempts WHERE establishment_id = ?1)",
    )
    .bind(&shop.id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(writes, (0, 0, 0));

    input["catalog_version"] = json!(published_version);
    let confirmed = checkout(&input, fresh, Some(&zone));
    let receipt = place(&app, &shop, &confirmed, key).await;
    let tracking = expect_json(
        get_public(&app, &tracking_api(&shop, &receipt), None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(tracking["totals"]["delivery_fee_centavos"], 0);
    assert_eq!(tracking["payment"]["amount_centavos"], 5_000);
    assert_eq!(order_count(&h, &shop).await, 1);
}

#[tokio::test]
async fn delivery_quotes_atomic_checkout_idempotency_and_public_tracking_are_tenant_scoped() {
    let h = harness(false).await;
    let app = preview_router(&h).await;
    let shop = Shop::create(&h, "delivery-checkout-alpha", 6_000).await;
    let other = Shop::create(&h, "delivery-checkout-beta", 0).await;
    for tenant in [&shop, &other] {
        grant_preview(&h, tenant).await;
    }
    let zone = shop.create_zone(&h, &app).await;
    let other_zone = other.create_zone(&h, &app).await;
    shop.publish(&app).await;
    other.publish(&app).await;
    let version = shop.catalog_version(&h).await;
    let mut input = shop.quote(version, Some(&zone), None);

    assert_eq!(
        post_json(
            &app,
            &shop.api("quotes"),
            &input,
            "https://outside.example.test",
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let mut wrong_zone = input.clone();
    wrong_zone["zone_id"] = json!(other_zone);
    let denied = expect_json(
        post_json(&app, &shop.api("quotes"), &wrong_zone, ORIGIN, None).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(denied["code"], "delivery.zone_not_served");
    let foreign_catalog_item = other.quote(version, Some(&zone), None);
    let unavailable = quote(&app, &shop, &foreign_catalog_item).await;
    assert_eq!(unavailable["lines"][0]["available"], false);
    assert!(unavailable["total_centavos"].is_null());

    input["lines"][0]["unit_price_centavos"] = json!(1);
    let priced = quote(&app, &shop, &input).await;
    assert_eq!(priced["items_subtotal_centavos"], 4_500);
    assert_eq!(priced["discount_centavos"], 0);
    assert_eq!(priced["delivery_fee_centavos"], 500);
    assert_eq!(priced["total_centavos"], 5_000);
    assert_eq!(priced["minimum_order_centavos"], 3_000);
    assert_eq!(priced["open_now"], true);
    assert_eq!(priced["issues"], json!([]));

    let valid = checkout(&input, &priced, Some(&zone));
    let mut missing_breakdown = valid.clone();
    missing_breakdown
        .as_object_mut()
        .unwrap()
        .remove("expected_totals");
    assert_eq!(
        post_json(
            &app,
            &shop.api("orders"),
            &missing_breakdown,
            ORIGIN,
            Some(Uuid::new_v4())
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    let mut wrong_breakdown = valid.clone();
    wrong_breakdown["expected_totals"]["delivery_fee_centavos"] = json!(600);
    wrong_breakdown["expected_totals"]["discount_centavos"] = json!(100);
    let key = Uuid::new_v4();
    let repriced = expect_json(
        post_json(
            &app,
            &shop.api("orders"),
            &wrong_breakdown,
            ORIGIN,
            Some(key),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(repriced["code"], "commerce.checkout_repriced");
    assert_eq!(repriced["quote"]["total_centavos"], 5_000);
    assert_eq!(repriced["quote"]["delivery_fee_centavos"], 500);
    assert_eq!(order_count(&h, &shop).await, 0);

    let receipt = place(&app, &shop, &valid, key).await;
    assert_eq!(place(&app, &shop, &valid, key).await, receipt);
    let mut edited = valid.clone();
    edited["lines"][0]["note"] = json!("Alterou a seleção");
    let conflict = expect_json(
        post_json(&app, &shop.api("orders"), &edited, ORIGIN, Some(key)).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(conflict["code"], "commerce.idempotency_mismatch");
    assert_eq!(order_count(&h, &shop).await, 1);
    assert_eq!(order_count(&h, &other).await, 0);

    let tracking_path = tracking_api(&shop, &receipt);
    let response = get_public(&app, &tracking_path, None).await;
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    let etag = response.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    let tracking = expect_json(response, StatusCode::OK).await;
    assert_eq!(tracking["state"], "placed");
    assert_eq!(tracking["can_cancel"], true);
    assert_eq!(tracking["totals"]["total_centavos"], 5_000);
    assert_private_response(&tracking, &receipt);
    assert_eq!(
        get_public(&app, &tracking_path, Some(&etag)).await.status(),
        StatusCode::NOT_MODIFIED
    );
    let public_id = receipt["order_id"].as_str().unwrap();
    assert_eq!(
        get_public(&app, &other.api(&format!("orders/{public_id}")), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let cancel = format!("{tracking_path}/cancel");
    let credentials = json!({ "cancel_token": receipt["cancel_token"] });
    assert_eq!(
        post_json(
            &app,
            &cancel,
            &credentials,
            "https://outside.example.test",
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_json(
            &app,
            &cancel,
            &json!({ "cancel_token": "invalid" }),
            ORIGIN,
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let result = expect_json(
        post_json(&app, &cancel, &credentials, ORIGIN, None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(result["state"], "cancelled");
    assert_private_response(&result, &receipt);
    let refreshed = expect_json(
        get_public(&app, &tracking_path, Some(&etag)).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(refreshed["state"], "cancelled");
    assert_eq!(refreshed["can_cancel"], false);
}
