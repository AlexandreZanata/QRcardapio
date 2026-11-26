use axum::http::StatusCode;
use uuid::Uuid;

use crate::fixture::{Shop, checkout, expect_json, get_public, grant_preview, submit};
use crate::orders::{assert_private_response, place, quote, tracking_api};
use crate::support::harness;

#[tokio::test]
async fn preview_delivery_and_pickup_complete_without_confirming_cash_payment() {
    let h = harness(false).await;
    let pilot = Shop::create(&h, "delivery-completion-pilot", 0).await;
    let app = &h.app;
    grant_preview(&h, &pilot).await;
    let zone = pilot.create_zone(&h, app).await;
    pilot.publish(app).await;
    assert_eq!(
        get_public(app, &pilot.storefront(), None).await.status(),
        StatusCode::OK
    );

    let catalog_version = pilot.catalog_version(&h).await;
    for (fulfillment, selected_zone, fee) in
        [("delivery", Some(zone.as_str()), 500), ("pickup", None, 0)]
    {
        let input = pilot.quote(catalog_version, selected_zone, None);
        let priced = quote(app, &pilot, &input).await;
        assert_eq!(priced["delivery_fee_centavos"], fee);
        let request = checkout(&input, &priced, selected_zone);
        let receipt = place(app, &pilot, &request, Uuid::new_v4()).await;
        let path = tracking_api(&pilot, &receipt);
        let mut tracking = expect_json(get_public(app, &path, None).await, StatusCode::OK).await;
        assert_eq!(tracking["fulfillment"], fulfillment);
        assert_eq!(tracking["state"], "placed");
        assert_eq!(tracking["payment"]["method"], "cash");
        assert_eq!(tracking["payment"]["state"], "pending");
        assert_eq!(
            tracking["payment"]["amount_centavos"],
            priced["total_centavos"]
        );

        let staff_order_id: String = sqlx::query_scalar(
            "SELECT id FROM orders WHERE establishment_id = ? AND public_id = ?",
        )
        .bind(&pilot.id)
        .bind(receipt["order_id"].as_str().unwrap())
        .fetch_one(&h.pool)
        .await
        .unwrap();
        let transition = format!("/dashboard/delivery/orders/{staff_order_id}/transition");
        for state in ["accepted", "preparing", "ready", "completed"] {
            let version = tracking["version"].as_i64().unwrap();
            let form = format!("to={state}&expected_version={version}&prep_minutes=15");
            assert_eq!(
                submit(app, &pilot.owner, &transition, &form).await.status(),
                StatusCode::SEE_OTHER,
                "{fulfillment}: {state}"
            );
            tracking = expect_json(get_public(app, &path, None).await, StatusCode::OK).await;
            assert_eq!(tracking["state"], state);
            assert_eq!(tracking["version"], version + 1);
            assert_eq!(tracking["payment"]["state"], "pending");
        }
        assert!(tracking["completed_at"].is_string());
        assert_eq!(tracking["can_cancel"], false);
        assert_private_response(&tracking, &receipt);
    }

    let recorded: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT o.fulfillment, o.state, p.method, p.state FROM orders o
         JOIN order_payments p ON p.establishment_id = o.establishment_id AND p.order_id = o.id
         WHERE o.establishment_id = ? ORDER BY o.fulfillment",
    )
    .bind(&pilot.id)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(
        recorded,
        ["delivery", "pickup"].map(|mode| {
            (
                mode.to_owned(),
                "completed".to_owned(),
                "cash".to_owned(),
                "pending".to_owned(),
            )
        }),
    );
}
