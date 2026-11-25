#[path = "commerce/checkout_fixtures.rs"]
mod checkout;
#[path = "commerce/checkout_regressions.rs"]
mod checkout_regressions;
#[path = "commerce/checkout_transaction.rs"]
mod checkout_transaction;
#[path = "commerce/order_fixtures.rs"]
mod fixture;
#[path = "commercial/support.rs"]
mod support;

use checkout::{assert_counts, placed};
use chrono::{Duration, NaiveDate};
use uuid::Uuid;

use qrcardapio::application::commerce::checkout::{place_order, quote};
use qrcardapio::domain::commerce::business_hours::{ServiceMode, Windows};
use qrcardapio::domain::commerce::checkout::{CheckoutCart, CheckoutOutcome};
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::queries::{
    business_hours, catalog_snapshots, checkout_attempts, customer_privacy, domain_events,
    product_variations,
};

#[tokio::test]
async fn concurrent_replays_return_same_order_and_payment_without_recreating_erased_profile() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-replay").await;
    let now = fixture::at("2026-11-24T05:30:00Z");
    let draft = fixture::catalog(&harness, &est, now).await;
    checkout::enable(pool, &est).await;
    let cart = CheckoutCart {
        catalog_version: draft.catalog_version,
        items: draft.items,
    };
    let target = checkout::target(&est);
    let key = Uuid::new_v4();
    let (one, two) = tokio::join!(
        place_order(pool, target, checkout::request(key, &cart), now),
        place_order(pool, target, checkout::request(key, &cart), now),
    );
    let receipt = placed(one.unwrap());
    assert_eq!(receipt, placed(two.unwrap()));
    assert_counts(pool, 1).await;

    let row: (i64, NaiveDate, String, i64, String, String) = sqlx::query_as(
        "SELECT o.total_centavos, o.business_date, o.acceptance_deadline_at,
                p.amount_centavos, p.state, c.customer_id
         FROM orders o JOIN order_payments p ON p.order_id = o.id AND p.establishment_id = o.establishment_id
         JOIN order_customers c ON c.order_id = o.id AND c.establishment_id = o.establishment_id
         WHERE o.public_id = ? AND o.establishment_id = ?",
    ).bind(&receipt.public_id).bind(&est).fetch_one(pool).await.unwrap();
    assert_eq!(row.0, 4_775);
    assert_eq!(row.1, NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());
    assert_eq!(fixture::at(&row.2), now + Duration::minutes(12));
    assert_eq!((row.3, row.4.as_str()), (4_775, "pending"));
    let private_snapshot: (String, String, String) = sqlx::query_as(
        "SELECT recipient_name, recipient_phone, cep FROM order_delivery_details WHERE establishment_id = ?",
    ).bind(&est).fetch_one(pool).await.unwrap();
    assert_eq!(
        private_snapshot,
        (
            "Cliente Checkout".into(),
            "+5565999001234".into(),
            "78000999".into()
        )
    );
    let events = domain_events::list_for_establishment(pool, &est)
        .await
        .unwrap();
    assert_eq!(events[0].event_type, "OrderCreated");
    for secret in [
        "Cliente Checkout",
        "+5565999001234",
        "Rua Particular",
        &key.to_string(),
    ] {
        assert!(!events[0].payload.contains(secret));
        assert!(!events[0].correlation_id.contains(secret));
    }

    let mut mismatch = checkout::request(key, &cart);
    mismatch.customer.phone_e164 = "dados alterados".into();
    assert!(matches!(
        place_order(pool, target, mismatch, now).await,
        Err(AppError::Validation("commerce.idempotency_mismatch"))
    ));

    customer_privacy::erase(pool, &est, &row.5, &now.to_rfc3339())
        .await
        .unwrap();
    business_hours::set_pause(pool, &est, true, None)
        .await
        .unwrap();
    let mut changed_prices = target;
    changed_prices.discount = None;
    changed_prices.delivery_fee_centavos = 1_000;
    assert_eq!(
        receipt,
        placed(
            place_order(pool, changed_prices, checkout::request(key, &cart), now)
                .await
                .unwrap()
        )
    );
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customers WHERE establishment_id = ? AND erased_at IS NULL",
    )
    .bind(&est)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(active, 0);
}

#[tokio::test]
async fn checkout_reprices_current_published_lines_and_preserves_unavailable_entries() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-reprice").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let draft = fixture::catalog(&harness, &est, now).await;
    checkout::enable(pool, &est).await;
    let cart = CheckoutCart {
        catalog_version: draft.catalog_version,
        items: draft.items,
    };
    let key = Uuid::new_v4();
    let line = &cart.items[0];
    let mut variation = fixture::variation(line.variation_id.as_deref().unwrap());
    variation.price_centavos = 3_000;
    product_variations::save(pool, &est, &line.product_id, &variation, &now.to_rfc3339())
        .await
        .unwrap();
    let target = checkout::target(&est);
    let before_publish = quote(pool, target, &cart, now).await.unwrap();
    assert_eq!(before_publish.totals.unwrap().total_centavos, 4_775);

    let latest = fixture::publish(pool, &est).await;
    let CheckoutOutcome::Repriced(fresh) =
        place_order(pool, target, checkout::request(key, &cart), now)
            .await
            .unwrap()
    else {
        panic!("stale cart was placed");
    };
    assert_eq!(fresh.catalog_version, latest.version);
    assert_eq!(fresh.lines[0].unwrap().line_total_centavos, 6_500);
    assert_eq!(fresh.totals.unwrap().total_centavos, 6_675);

    let mut revalidated = cart.clone();
    revalidated.catalog_version = latest.version;
    assert_eq!(
        place_order(pool, target, checkout::request(key, &revalidated), now)
            .await
            .unwrap(),
        CheckoutOutcome::Repriced(fresh)
    );
    let sold_out = catalog_snapshots::publish_product_availability(
        pool,
        &est,
        fixture::CATALOG_CHANNEL,
        &line.product_id,
        false,
        &now.to_rfc3339(),
    )
    .await
    .unwrap();
    revalidated.catalog_version = sold_out.version;
    let CheckoutOutcome::Repriced(unavailable) =
        place_order(pool, target, checkout::request(key, &revalidated), now)
            .await
            .unwrap()
    else {
        panic!("unavailable cart was placed");
    };
    assert_eq!(unavailable.catalog_version, sold_out.version);
    assert_eq!(unavailable.lines, [None]);
    assert_eq!(unavailable.totals, None);
    assert_counts(pool, 0).await;
}

#[tokio::test]
async fn schedule_minimum_and_failed_payment_leave_no_partial_checkout() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-atomic").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let draft = fixture::catalog(&harness, &est, now).await;
    let mut settings = checkout::enable(pool, &est).await;
    let cart = CheckoutCart {
        catalog_version: draft.catalog_version,
        items: draft.items,
    };
    let target = checkout::target(&est);
    let key = Uuid::new_v4();
    let date = now.date_naive();

    business_hours::save_exception(pool, &est, date, &Windows::default())
        .await
        .unwrap();
    assert!(!quote(pool, target, &cart, now).await.unwrap().open_now);
    assert!(matches!(
        place_order(pool, target, checkout::request(key, &cart), now).await,
        Err(AppError::Validation("delivery.closed"))
    ));
    business_hours::delete_exception(pool, &est, date)
        .await
        .unwrap();
    settings.delivery = false;
    business_hours::save_settings(pool, &est, &settings)
        .await
        .unwrap();
    assert!(matches!(
        place_order(pool, target, checkout::request(key, &cart), now).await,
        Err(AppError::Validation("delivery.mode_unavailable"))
    ));
    settings.delivery = true;
    settings.minimum_centavos = 4_501;
    business_hours::save_settings(pool, &est, &settings)
        .await
        .unwrap();
    assert!(matches!(
        place_order(pool, target, checkout::request(key, &cart), now).await,
        Err(AppError::Validation("delivery.below_minimum"))
    ));

    settings.minimum_centavos = 4_500;
    business_hours::save_settings(pool, &est, &settings)
        .await
        .unwrap();
    let mut invalid = checkout::request(key, &cart);
    invalid.payment.change_for_centavos = Some(4_774);
    assert!(matches!(
        place_order(pool, target, invalid, now).await,
        Err(AppError::Validation("commerce.invalid_cash_change"))
    ));
    assert_counts(pool, 0).await;
    placed(
        place_order(pool, target, checkout::request(key, &cart), now)
            .await
            .unwrap(),
    );
    assert_counts(pool, 1).await;
}

#[tokio::test]
async fn independent_tenant_keys_expire_and_pruning_preserves_orders() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, a) = support::owner(pool, "checkout-tenant-a").await;
    let (_, b) = support::owner(pool, "checkout-tenant-b").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let key = Uuid::new_v4();
    let mut receipts = Vec::new();
    let mut carts = Vec::new();
    for id in [&a, &b] {
        let draft = fixture::catalog(&harness, id, now).await;
        checkout::enable(pool, id).await;
        let cart = CheckoutCart {
            catalog_version: draft.catalog_version,
            items: draft.items,
        };
        receipts.push(placed(
            place_order(
                pool,
                checkout::target(id),
                checkout::request(key, &cart),
                now,
            )
            .await
            .unwrap(),
        ));
        carts.push(cart);
    }
    assert_ne!(receipts[0], receipts[1]);
    assert_counts(pool, 2).await;

    let mut foreign = carts[0].clone();
    foreign.items[0]
        .product_id
        .clone_from(&carts[1].items[0].product_id);
    let foreign_quote = quote(pool, checkout::target(&a), &foreign, now)
        .await
        .unwrap();
    foreign.items[0].product_id = "unknown".into();
    assert_eq!(
        foreign_quote,
        quote(pool, checkout::target(&a), &foreign, now)
            .await
            .unwrap()
    );

    let expiry = now + Duration::days(7);
    assert_eq!(
        checkout_attempts::prune_expired(pool, expiry - Duration::seconds(1), 3)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        checkout_attempts::prune_expired(pool, expiry, 1)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        checkout_attempts::prune_expired(pool, expiry, 0)
            .await
            .unwrap(),
        0
    );

    let mut pickup = checkout::target(&a);
    pickup.mode = ServiceMode::Pickup;
    let mut request = checkout::request(key, &carts[0]);
    request.address = None;
    request.expected_total_centavos = 4_275;
    let new = placed(place_order(pool, pickup, request, expiry).await.unwrap());
    assert_ne!(new, receipts[0]);
    let renewed = placed(
        place_order(
            pool,
            checkout::target(&b),
            checkout::request(key, &carts[1]),
            expiry,
        )
        .await
        .unwrap(),
    );
    assert_ne!(renewed, receipts[1]);
    assert_eq!(
        checkout_attempts::prune_expired(pool, expiry, 5)
            .await
            .unwrap(),
        0
    );
    let (orders, details): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM orders), (SELECT COUNT(*) FROM order_delivery_details)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!((orders, details), (4, 3));
}
