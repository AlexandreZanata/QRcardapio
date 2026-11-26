use qrcardapio::application::commerce::checkout::{place_order, place_order_in, quote};
use qrcardapio::domain::commerce::checkout::{
    CheckoutCart, CheckoutOutcome, CheckoutQuoteIssue, CheckoutTarget,
};
use qrcardapio::domain::commerce::discount::Discount;
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::begin_immediate;
use qrcardapio::infrastructure::database::queries::{customer_privacy, product_variations};
use uuid::Uuid;

use crate::{checkout, fixture, support};

#[tokio::test]
async fn quote_breakdown_changes_reprice_even_when_the_total_does_not_change() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-breakdown").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    checkout::enable(pool, &est).await;
    let sample = fixture::catalog(&harness, &est, now).await;
    let cart = CheckoutCart {
        catalog_version: sample.catalog_version,
        items: sample.items,
    };
    let original = checkout::target(&est);
    let quoted = quote(pool, original, &cart, now)
        .await
        .unwrap()
        .totals
        .unwrap();
    let revised = CheckoutTarget {
        discount: Some(Discount::FixedCentavos(quoted.discount_centavos + 100)),
        delivery_fee_centavos: quoted.delivery_fee_centavos + 100,
        ..original
    };
    let fresh = quote(pool, revised, &cart, now).await.unwrap();
    let totals = fresh.totals.unwrap();
    assert_ne!(totals, quoted);
    assert_eq!(totals.total_centavos, quoted.total_centavos);
    assert_eq!(fresh.catalog_version, cart.catalog_version);
    let key = Uuid::new_v4();
    let request = |expected| {
        let mut request = checkout::request(key, &cart);
        request.expected_totals = Some(expected);
        request
    };

    let repriced = CheckoutOutcome::Repriced(fresh);
    assert_eq!(
        place_order(pool, revised, request(quoted), now)
            .await
            .unwrap(),
        repriced
    );
    let mut tx = begin_immediate(pool).await.unwrap();
    let stale = place_order_in(&mut tx, revised, request(quoted), now)
        .await
        .unwrap();
    assert_eq!(stale.outcome, repriced);
    assert!(stale.new_order_id.is_none());
    tx.commit().await.unwrap();
    checkout::assert_counts(pool, 0).await;

    let mut tx = begin_immediate(pool).await.unwrap();
    let placed = place_order_in(&mut tx, revised, request(totals), now)
        .await
        .unwrap();
    assert!(placed.new_order_id.is_some());
    let retried = place_order_in(&mut tx, revised, request(totals), now)
        .await
        .unwrap();
    assert_eq!(retried.outcome, placed.outcome);
    assert!(retried.new_order_id.is_none());
    tx.commit().await.unwrap();
    assert_eq!(
        place_order(pool, original, request(totals), now)
            .await
            .unwrap(),
        placed.outcome
    );
    assert!(matches!(
        place_order(pool, revised, request(quoted), now).await,
        Err(AppError::Validation("commerce.idempotency_mismatch"))
    ));
    checkout::assert_counts(pool, 1).await;
}

#[tokio::test]
async fn checkout_exports_then_erases_free_form_delivery_neighborhood() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-neighborhood-privacy").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    checkout::enable(pool, &est).await;
    let sample = fixture::catalog(&harness, &est, now).await;
    let cart = CheckoutCart {
        catalog_version: sample.catalog_version,
        items: sample.items,
    };
    let target = checkout::target(&est);
    let text = "Centro, contato +5565999001234";
    let mut request = checkout::request(Uuid::new_v4(), &cart);
    request.address.as_mut().unwrap().neighborhood = text.into();
    checkout::placed(place_order(pool, target, request, now).await.unwrap());

    let customer: String =
        sqlx::query_scalar("SELECT customer_id FROM order_customers WHERE establishment_id = ?")
            .bind(&est)
            .fetch_one(pool)
            .await
            .unwrap();
    let export = customer_privacy::export(pool, &est, &customer)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(export).unwrap()["orders"][0]["delivery"]["neighborhood"],
        text
    );

    customer_privacy::erase(pool, &est, &customer, &now.to_rfc3339())
        .await
        .unwrap();
    let (label, private_rows): (String, i64) = sqlx::query_as(
        "SELECT neighborhood_label, (SELECT COUNT(*) FROM order_delivery_neighborhoods n
          WHERE n.establishment_id = d.establishment_id AND n.order_id = d.order_id)
         FROM order_delivery_details d WHERE d.establishment_id = ? AND d.erased_at IS NOT NULL",
    )
    .bind(&est)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(label.is_empty());
    assert_eq!(private_rows, 0);
}

#[tokio::test]
async fn stale_cart_repricing_explains_the_current_order_limit_without_writes() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-cap-reprice").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    checkout::enable(pool, &est).await;
    let sample = fixture::catalog(&harness, &est, now).await;
    let cart = CheckoutCart {
        catalog_version: sample.catalog_version,
        items: sample.items,
    };
    let target = checkout::target(&est);
    let line = &cart.items[0];
    let mut price = fixture::variation(line.variation_id.as_deref().unwrap());
    price.price_centavos = 600_000;
    product_variations::save(pool, &est, &line.product_id, &price, &now.to_rfc3339())
        .await
        .unwrap();
    let published = fixture::publish(pool, &est).await;

    let request = checkout::request(Uuid::new_v4(), &cart);
    let CheckoutOutcome::Repriced(fresh) = place_order(pool, target, request, now).await.unwrap()
    else {
        panic!("cart above order limit was placed");
    };
    assert_eq!(fresh.catalog_version, published.version);
    assert_eq!(fresh.lines[0].unwrap().line_total_centavos, 1_200_500);
    assert_eq!(fresh.totals, None);
    assert_eq!(fresh.issue, Some(CheckoutQuoteIssue::OrderLimit));
    assert_eq!(
        fresh.issue.unwrap().i18n_key(),
        "commerce.invalid_order_price"
    );
    assert_eq!(quote(pool, target, &cart, now).await.unwrap(), fresh);
    checkout::assert_counts(pool, 0).await;
}
