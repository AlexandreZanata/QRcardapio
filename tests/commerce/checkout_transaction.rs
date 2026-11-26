use std::time::Duration;

use aws_lc_rs::hmac;
use qrcardapio::application::commerce::checkout::{place_order, place_order_in};
use qrcardapio::domain::commerce::checkout::CheckoutCart;
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::begin_immediate;
use uuid::Uuid;

use crate::{checkout, fixture, support};

#[tokio::test]
async fn committed_checkout_can_replay_while_another_writer_holds_the_database() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-read-replay").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    checkout::enable(pool, &est).await;
    let sample = fixture::catalog(&harness, &est, now).await;
    let cart = CheckoutCart {
        catalog_version: sample.catalog_version,
        items: sample.items,
    };
    let target = checkout::target(&est);
    let key = Uuid::new_v4();
    let request = checkout::request(key, &cart);
    let old_payload = serde_json::to_vec(&(
        "commerce.checkout.v1",
        target.catalog_channel,
        "delivery",
        &request.cart,
        request.expected_total_centavos,
        &request.customer,
        &request.address,
        request.payment,
    ))
    .unwrap();
    let old_key = hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes());
    let old_digest = hmac::sign(&old_key, &old_payload);
    let saved = place_order(pool, target, request, now).await.unwrap();
    let stored_digest: Vec<u8> =
        sqlx::query_scalar("SELECT request_hash FROM checkout_attempts WHERE establishment_id = ?")
            .bind(&est)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(stored_digest, old_digest.as_ref());

    // Keep an established reader available so connection setup pragmas do not
    // compete with the writer lock this test uses to check the replay query.
    let reader = pool.acquire().await.unwrap();
    let writer = begin_immediate(pool).await.unwrap();
    drop(reader);
    let retry = tokio::time::timeout(
        Duration::from_secs(2),
        place_order(pool, target, checkout::request(key, &cart), now),
    )
    .await
    .expect("a committed replay must not wait for the SQLite writer")
    .unwrap();
    writer.rollback().await.unwrap();

    assert_eq!(retry, saved);
    checkout::assert_counts(pool, 1).await;
}

#[tokio::test]
async fn caller_transaction_can_roll_back_placement_and_savepoint_isolates_errors() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, est) = support::owner(pool, "checkout-savepoint").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    checkout::enable(pool, &est).await;
    let sample = fixture::catalog(&harness, &est, now).await;
    let cart = CheckoutCart {
        catalog_version: sample.catalog_version,
        items: sample.items,
    };
    let target = checkout::target(&est);
    let key = Uuid::new_v4();

    let mut tx = begin_immediate(pool).await.unwrap();
    let first = place_order_in(&mut tx, target, checkout::request(key, &cart), now)
        .await
        .unwrap();
    assert!(first.new_order_id.is_some());
    tx.rollback().await.unwrap();
    checkout::assert_counts(pool, 0).await;

    let mut tx = begin_immediate(pool).await.unwrap();
    let mut invalid = checkout::request(key, &cart);
    invalid.payment.change_for_centavos = Some(1);
    assert!(matches!(
        place_order_in(&mut tx, target, invalid, now).await,
        Err(AppError::Validation("commerce.invalid_cash_change"))
    ));
    // The surrounding transaction can commit safely even if checkout failed.
    tx.commit().await.unwrap();
    checkout::assert_counts(pool, 0).await;

    let mut tx = begin_immediate(pool).await.unwrap();
    let saved = place_order_in(&mut tx, target, checkout::request(key, &cart), now)
        .await
        .unwrap();
    assert!(saved.new_order_id.is_some());
    let retry = place_order_in(&mut tx, target, checkout::request(key, &cart), now)
        .await
        .unwrap();
    assert_eq!(retry.outcome, saved.outcome);
    assert!(retry.new_order_id.is_none());
    tx.commit().await.unwrap();
    checkout::assert_counts(pool, 1).await;
}
