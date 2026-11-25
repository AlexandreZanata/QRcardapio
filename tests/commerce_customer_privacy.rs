#[path = "commerce/order_fixtures.rs"]
mod fixture;
#[path = "commercial/support.rs"]
mod support;

use axum::http::{StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;

use qrcardapio::application::commerce::order_creation;
use support::{get, harness, location, manager, owner, post_form};

#[tokio::test]
async fn customer_export_and_erasure_are_owner_only_and_retain_order_facts() {
    let h = harness(false).await;
    let (owner_a, est) = owner(&h.pool, "privacy-a").await;
    let (owner_b, other) = owner(&h.pool, "privacy-b").await;
    let manager = manager(&h.pool, &est, "privacy-manager@example.test").await;
    let registration = format!(
        "_csrf={}&name=Customer+Sample&phone_e164=%2865%29+99999-0020&email=sample%40example.test",
        owner_a.csrf_token
    );
    let new = post_form(
        &h.app,
        &owner_a,
        "/dashboard/customers",
        registration.clone(),
    )
    .await;
    assert_eq!(new.status(), StatusCode::SEE_OTHER);
    let detail_path = location(&new);
    let customer = detail_path.rsplit('/').next().unwrap();
    assert_eq!(
        post_form(
            &h.app,
            &owner_a,
            "/dashboard/customers",
            registration.clone()
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let other_res = post_form(
        &h.app,
        &owner_b,
        "/dashboard/customers",
        format!(
            "_csrf={}&name=Other+Name&phone_e164=%2B5565999990020",
            owner_b.csrf_token
        ),
    )
    .await;
    assert_eq!(other_res.status(), StatusCode::SEE_OTHER);
    let other_path = location(&other_res);
    assert_eq!(
        get(&h.app, &owner_b, &detail_path).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&h.app, &manager, &detail_path).await.status(),
        StatusCode::OK
    );

    let address_path = format!("{detail_path}/addresses");
    let address = format!(
        "_csrf={}&street=Rua+Sul&street_number=10&neighborhood=Centro&city=Cuiaba&uf=MT&cep=78000-001",
        owner_a.csrf_token
    );
    assert_eq!(
        post_form(&h.app, &owner_a, &address_path, address)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let address = format!(
        "_csrf={}&street=Rua+Norte&street_number=39&neighborhood=Centro&city=Cuiaba&uf=MT&cep=78000-002",
        owner_b.csrf_token
    );
    assert_eq!(
        post_form(&h.app, &owner_b, &address_path, address)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let instant = fixture::at("2026-11-25T15:00:00Z");
    let command = fixture::catalog(&h, &est, instant).await;
    let order = order_creation::create(&h.pool, &est, &command, instant)
        .await
        .unwrap();
    assert!(
        sqlx::query(
            "INSERT INTO order_customers (establishment_id, order_id, customer_id) VALUES (?, ?, ?)"
        )
        .bind(&est)
        .bind(&order.id)
        .bind(other_path.rsplit('/').next().unwrap())
        .execute(&h.pool)
        .await
        .is_err()
    );
    sqlx::query(
        "INSERT INTO order_customers (establishment_id, order_id, customer_id) VALUES (?, ?, ?)",
    )
    .bind(&est)
    .bind(&order.id)
    .bind(customer)
    .execute(&h.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO order_delivery_details (establishment_id, order_id, recipient_name, recipient_phone, street, neighborhood_label) VALUES (?, ?, 'Customer Sample', '+5565999990020', 'Rua Sul', 'Centro')")
        .bind(&est).bind(&order.id).execute(&h.pool).await.unwrap();
    let export_path = format!("{detail_path}/export");
    assert_eq!(
        post_form(
            &h.app,
            &manager,
            &export_path,
            format!("_csrf={}", manager.csrf_token)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_form(
            &h.app,
            &owner_b,
            &export_path,
            format!("_csrf={}", owner_b.csrf_token)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let response = post_form(
        &h.app,
        &owner_a,
        &export_path,
        format!("_csrf={}", owner_a.csrf_token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(
        response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    let export: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(export["customer"]["phone_e164"], "+5565999990020");
    assert_eq!(export["addresses"][0]["cep"], "78000001");
    assert_eq!(export["orders"][0]["public_id"], order.public_id);
    assert_eq!(export["orders"][0]["delivery"]["street"], "Rua Sul");
    assert_eq!(
        export["orders"][0]["items"][0]["extras"][0]["name"],
        "Queijo"
    );

    let erase_path = format!("{detail_path}/erase");
    assert_eq!(
        post_form(
            &h.app,
            &manager,
            &erase_path,
            format!("_csrf={}&confirm=1", manager.csrf_token)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_form(
            &h.app,
            &owner_a,
            &erase_path,
            format!("_csrf={}", owner_a.csrf_token)
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post_form(
            &h.app,
            &owner_a,
            &erase_path,
            format!("_csrf={}&confirm=1", owner_a.csrf_token)
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get(&h.app, &owner_a, &detail_path).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&h.app, &owner_b, &other_path).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        post_form(&h.app, &owner_a, "/dashboard/customers", registration)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let retained: (i64, String, Option<String>, Option<String>) = sqlx::query_as("SELECT o.total_centavos, d.neighborhood_label, d.recipient_phone, d.street FROM orders o JOIN order_delivery_details d ON d.order_id = o.id AND d.establishment_id = o.establishment_id WHERE o.id = ? AND o.establishment_id = ?")
        .bind(&order.id).bind(est).fetch_one(&h.pool).await.unwrap();
    assert_eq!(
        retained,
        (order.total_centavos, "Centro".into(), None, None)
    );
    let link_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM order_customers WHERE order_id = ?")
            .bind(&order.id)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(link_count, 0);
    let addresses: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM customer_addresses WHERE customer_id = ?")
            .bind(customer)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(addresses, 0);
    let other_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customers WHERE establishment_id = ? AND erased_at IS NULL",
    )
    .bind(other)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(other_count, 1);
}
