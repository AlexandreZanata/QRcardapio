use qrcardapio::application::commerce::checkout::place_order_in;
use qrcardapio::domain::commerce::checkout::CheckoutCart;
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::begin_immediate;
use uuid::Uuid;

use crate::{checkout, fixture, support};

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
