use axum::http::{StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;

use qrcardapio::infrastructure::database::queries::{
    delivery_orders, order_customers, order_payments,
};

use crate::fixture::{PHONE, Shop, body, checkout, expect_json, get_public, grant_preview, submit};
use crate::orders::{place, quote, tracking_api};
use crate::support::{Harness, get, harness};

const ORDERS: &str = "/dashboard/delivery/orders";
const ORDER_FRAGMENT: &str = "/dashboard/delivery/orders?fragment=1";
const BLOCK_CUSTOMER: &str = "/dashboard/delivery/customers/block";
const BUYER: &str = "Synthetic Delivery Buyer";
const OTHER_BUYER: &str = "Synthetic Other Buyer";
const STREET: &str = "Synthetic Buyer Street";
const COMPLEMENT: &str = "Synthetic Apartment";
const REFERENCE: &str = "Synthetic Reference";
const ORDER_NOTE: &str = "Synthetic private order note";
const LINE_NOTE: &str = "Synthetic private line note";
const BLOCK_REASON: &str = "Synthetic private block reason";
const OTHER_REASON: &str = "Synthetic other block reason";
const NO_PROFILE_PHONE: &str = "+5565999998765";

struct PlacedOrder {
    id: String,
    checkout_key: Uuid,
    receipt: Value,
}

#[tokio::test]
async fn delivery_customer_export_and_erasure_preserve_the_financial_ledger_without_pii() {
    let h = harness(false).await;
    let shop = Shop::create(&h, "delivery-lgpd-target", 0).await;
    let other = Shop::create(&h, "delivery-lgpd-other", 0).await;
    for tenant in [&shop, &other] {
        grant_preview(&h, tenant).await;
    }
    let zone = shop.create_zone(&h, &h.app).await;
    shop.publish(&h.app).await;
    other.publish(&h.app).await;

    let delivered = buy(&h, &shop, Some(&zone), BUYER).await;
    let pickup = buy(&h, &shop, None, BUYER).await;
    let third = buy(&h, &shop, Some(&zone), BUYER).await;
    let other_order = buy(&h, &other, None, OTHER_BUYER).await;
    let customer = delivery_orders::get_customer_id_for_order(&h.pool, &shop.id, &delivered.id)
        .await
        .unwrap()
        .unwrap();
    let other_customer =
        delivery_orders::get_customer_id_for_order(&h.pool, &other.id, &other_order.id)
            .await
            .unwrap()
            .unwrap();
    assert_ne!(customer, other_customer);

    complete_and_refund(&h, &shop, &delivered).await;
    for (tenant, phone, reason) in [
        (&shop, PHONE, BLOCK_REASON),
        (&other, PHONE, OTHER_REASON),
        (&shop, NO_PROFILE_PHONE, "Unlinked block"),
    ] {
        let reason = reason.replace(' ', "+");
        assert_eq!(
            submit(
                &h.app,
                &tenant.owner,
                BLOCK_CUSTOMER,
                &format!("phone=%2B{}&block=1&reason={reason}", &phone[1..]),
            )
            .await
            .status(),
            StatusCode::SEE_OTHER
        );
    }

    let export_path = format!("/dashboard/customers/{customer}/export");
    assert_eq!(
        submit(&h.app, &other.owner, &export_path, "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = submit(&h.app, &shop.owner, &export_path, "").await;
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(
        response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    let exported = expect_json(response, StatusCode::OK).await;
    assert_eq!(exported["customer"]["name"], BUYER);
    assert_eq!(exported["customer"]["phone_e164"], PHONE);
    assert_eq!(exported["delivery_phone_block"]["blocked"], true);
    assert_eq!(exported["delivery_phone_block"]["reason"], BLOCK_REASON);
    assert!(exported["delivery_phone_block"]["blocked_at"].is_string());
    let exported_orders = exported["orders"].as_array().unwrap();
    assert_eq!(exported_orders.len(), 3);
    for order in [&delivered, &pickup, &third] {
        let entry = exported_orders
            .iter()
            .find(|value| value["public_id"] == order.receipt["order_id"])
            .unwrap();
        assert_eq!(entry["order_note"], ORDER_NOTE);
        assert_eq!(entry["items"][0]["note"], LINE_NOTE);
        assert_eq!(entry["items"][0]["extras"][0]["name"], "Queijo");
        if order.id == pickup.id {
            assert!(entry["delivery"].is_null());
        } else {
            assert_eq!(
                entry["delivery"],
                json!({
                    "recipient_name": BUYER,
                    "recipient_phone": PHONE,
                    "street": STREET,
                    "street_number": "77",
                    "complement": COMPLEMENT,
                    "neighborhood": "Centro",
                    "city": "Sao Paulo",
                    "uf": "SP",
                    "cep": "01001000",
                    "reference": REFERENCE
                })
            );
        }
    }

    let staff = get(&h.app, &shop.owner, ORDER_FRAGMENT).await;
    let staff_etag = staff.headers()[header::ETAG].clone();
    let staff_html = body(staff).await;
    for entered in [BUYER, PHONE, STREET, ORDER_NOTE, LINE_NOTE] {
        assert!(staff_html.contains(entered));
    }
    let retained = retained_ledger(&h, &shop).await;
    let event_rows = events(&h, &shop).await;
    assert_event_privacy(&event_rows, [&delivered, &pickup, &third]);

    let erase_path = format!("/dashboard/customers/{customer}/erase");
    assert_eq!(
        submit(&h.app, &other.owner, &erase_path, "confirm=1")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        submit(&h.app, &shop.owner, &erase_path, "confirm=1")
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        submit(&h.app, &shop.owner, &export_path, "").await.status(),
        StatusCode::NOT_FOUND
    );
    let erased: bool = sqlx::query_scalar(
        "SELECT erased_at IS NOT NULL AND name IS NULL AND phone_e164 IS NULL AND email IS NULL
         FROM customers WHERE establishment_id = ? AND id = ?",
    )
    .bind(&shop.id)
    .bind(&customer)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert!(erased);
    for order in [&delivered, &pickup, &third] {
        assert!(
            order_customers::get_contact(&h.pool, &shop.id, &order.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            delivery_orders::get_metadata(&h.pool, &shop.id, &order.id)
                .await
                .unwrap()
                .unwrap()
                .order_note
                .is_empty()
        );
        let items = delivery_orders::list_order_items(&h.pool, &shop.id, &order.id)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].note.is_empty());
        if order.id != pickup.id {
            assert_address_erased(&h, &shop, order).await;
        }
    }
    assert_eq!(retained_ledger(&h, &shop).await, retained);
    assert_eq!(events(&h, &shop).await, event_rows);
    let staff = get(&h.app, &shop.owner, ORDER_FRAGMENT).await;
    assert_ne!(staff.headers()[header::ETAG], staff_etag);
    let staff_html = body(staff).await;
    for erased in [
        BUYER, PHONE, STREET, COMPLEMENT, REFERENCE, ORDER_NOTE, LINE_NOTE,
    ] {
        assert!(!staff_html.contains(erased));
    }

    for (tenant, phone, blocked) in [
        (&shop, PHONE, false),
        (&other, PHONE, true),
        (&shop, NO_PROFILE_PHONE, true),
    ] {
        assert_eq!(
            delivery_orders::is_phone_blocked(&h.pool, &tenant.id, phone)
                .await
                .unwrap(),
            blocked
        );
    }
    let other_export = expect_json(
        submit(
            &h.app,
            &other.owner,
            &format!("/dashboard/customers/{other_customer}/export"),
            "",
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(other_export["customer"]["name"], OTHER_BUYER);
    assert_eq!(other_export["delivery_phone_block"]["reason"], OTHER_REASON);
    assert_eq!(other_export["orders"][0]["order_note"], ORDER_NOTE);
    assert_eq!(other_export["orders"][0]["items"][0]["note"], LINE_NOTE);
}

async fn buy(h: &Harness, shop: &Shop, zone: Option<&str>, name: &str) -> PlacedOrder {
    let input = shop.quote(shop.catalog_version(h).await, zone, None);
    let priced = quote(&h.app, shop, &input).await;
    let mut request = checkout(&input, &priced, zone);
    request["customer"]["name"] = json!(name);
    request["note"] = json!(ORDER_NOTE);
    request["lines"][0]["note"] = json!(LINE_NOTE);
    if zone.is_some() {
        request["address"]["street"] = json!(STREET);
        request["address"]["complement"] = json!(COMPLEMENT);
        request["address"]["reference"] = json!(REFERENCE);
    }
    let checkout_key = Uuid::new_v4();
    let receipt = place(&h.app, shop, &request, checkout_key).await;
    let id = delivery_orders::find_order_by_public_id(
        &h.pool,
        &shop.id,
        receipt["order_id"].as_str().unwrap(),
    )
    .await
    .unwrap()
    .unwrap()
    .id;
    PlacedOrder {
        id,
        checkout_key,
        receipt,
    }
}

async fn complete_and_refund(h: &Harness, shop: &Shop, order: &PlacedOrder) {
    let order_path = format!("{ORDERS}/{}", order.id);
    for (version, state) in (1..).zip(["accepted", "preparing", "ready", "completed"]) {
        assert_eq!(
            submit(
                &h.app,
                &shop.owner,
                &format!("{order_path}/transition"),
                &format!("to={state}&expected_version={version}&prep_minutes=15"),
            )
            .await
            .status(),
            StatusCode::SEE_OTHER
        );
    }
    let payment = order_payments::latest_for_order(&h.pool, &shop.id, &order.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        submit(
            &h.app,
            &shop.owner,
            &format!("{order_path}/payments/{}/confirm", payment.id),
            &format!("method=cash&expected_version={}", payment.version),
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        submit(
            &h.app,
            &shop.owner,
            &format!("{order_path}/refunds"),
            &format!(
                "payment_id={}&amount=2%2C50&reason_code=customer_request",
                payment.id
            ),
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    let tracking = expect_json(
        get_public(&h.app, &tracking_api(shop, &order.receipt), None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(tracking["state"], "completed");
    assert_eq!(tracking["payment"]["state"], "partially_refunded");
}

async fn assert_address_erased(h: &Harness, shop: &Shop, order: &PlacedOrder) {
    let (scrubbed, label): (bool, String) = sqlx::query_as(
        "SELECT erased_at IS NOT NULL AND recipient_name IS NULL AND recipient_phone IS NULL
            AND street IS NULL AND street_number IS NULL AND complement IS NULL
            AND city IS NULL AND uf IS NULL AND cep IS NULL AND reference IS NULL,
            neighborhood_label
         FROM order_delivery_details WHERE establishment_id = ? AND order_id = ?",
    )
    .bind(&shop.id)
    .bind(&order.id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert!(scrubbed);
    assert_eq!(label, "Centro — Sao Paulo");
}

async fn retained_ledger(h: &Harness, shop: &Shop) -> Value {
    let facts: String = sqlx::query_scalar(
        "SELECT json_object(
            'orders', json((SELECT json_group_array(json_array(o.id, o.total_centavos, o.state, o.placed_at) ORDER BY o.id)
                 FROM orders o WHERE o.establishment_id = e.id)),
            'items', json((SELECT json_group_array(json_array(i.id, i.order_id, i.name, i.unit_price_centavos, i.quantity) ORDER BY i.id)
                 FROM order_items i WHERE i.establishment_id = e.id)),
            'payments', json((SELECT json_group_array(json_array(p.id, p.order_id, p.amount_centavos, p.refunded_centavos, p.state) ORDER BY p.id)
                 FROM order_payments p WHERE p.establishment_id = e.id)),
            'refunds', json((SELECT json_group_array(json_array(r.id, r.payment_id, r.amount_centavos, r.reason_code) ORDER BY r.id)
                 FROM payment_refunds r WHERE r.establishment_id = e.id))
         ) FROM establishments e WHERE e.id = ?",
    )
    .bind(&shop.id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let ledger: Value = serde_json::from_str(&facts).unwrap();
    assert_eq!(ledger["orders"].as_array().unwrap().len(), 3);
    assert_eq!(ledger["items"].as_array().unwrap().len(), 3);
    assert_eq!(ledger["payments"].as_array().unwrap().len(), 3);
    assert_eq!(ledger["refunds"].as_array().unwrap().len(), 1);
    assert_eq!(ledger["refunds"][0][2], 250);
    ledger
}

async fn events(h: &Harness, shop: &Shop) -> Vec<(String, i64, String)> {
    sqlx::query_as(
        "SELECT event_type, event_version, payload FROM domain_events
         WHERE establishment_id = ? ORDER BY seq",
    )
    .bind(&shop.id)
    .fetch_all(&h.pool)
    .await
    .unwrap()
}

fn assert_event_privacy(events: &[(String, i64, String)], orders: [&PlacedOrder; 3]) {
    for kind in [
        "OrderAccepted",
        "OrderPreparing",
        "OrderReady",
        "OrderCompleted",
        "PaymentConfirmed",
        "PaymentRefunded",
    ] {
        assert_eq!(events.iter().filter(|event| event.0 == kind).count(), 1);
    }
    assert_eq!(
        events
            .iter()
            .filter(|event| event.0 == "OrderCreated")
            .count(),
        3
    );
    let registry: Value =
        serde_json::from_str(include_str!("../../contracts/events/registry.json")).unwrap();
    for (kind, version, raw) in events {
        let definition = registry["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["type"] == kind.as_str() && row["version"] == *version)
            .unwrap();
        let schema: Value = serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("contracts/events")
                    .join(definition["schema"].as_str().unwrap()),
            )
            .unwrap(),
        )
        .unwrap();
        let payload: Value = serde_json::from_str(raw).unwrap();
        let payload = payload.as_object().unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            payload
                .keys()
                .all(|key| schema["properties"].get(key.as_str()).is_some())
        );
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .all(|key| { payload.contains_key(key.as_str().unwrap()) })
        );
        for private in [
            BUYER,
            PHONE,
            &PHONE[1..],
            STREET,
            COMPLEMENT,
            REFERENCE,
            ORDER_NOTE,
            LINE_NOTE,
            BLOCK_REASON,
        ] {
            assert!(
                !raw.contains(private),
                "{kind} v{version} contains buyer data"
            );
        }
        for order in orders {
            assert!(!raw.contains(order.receipt["cancel_token"].as_str().unwrap()));
            assert!(!raw.contains(&order.checkout_key.to_string()));
        }
    }
}
