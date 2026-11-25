#[path = "commerce/order_fixtures.rs"]
mod fixture;
#[path = "commerce/payment_compensation.rs"]
mod payment_compensation;
#[path = "commerce/payment_isolation.rs"]
mod payment_isolation;
#[path = "commercial/support.rs"]
mod support;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

use qrcardapio::application::commerce::order_creation;
use qrcardapio::application::commerce::order_transitions;
use qrcardapio::application::commerce::payments::{cancel, confirm, create_pending, refund};
use qrcardapio::domain::commerce::order::{Order, OrderAction, TransitionOrder};
use qrcardapio::domain::commerce::order_payment::{
    OfflineMethod, OfflinePaymentChoice, OrderPayment, PaymentOperation, RefundReason,
};
use qrcardapio::domain::commerce::state_machine::{
    Actor, Machine, OrderPaymentState, OrderState, definition,
};
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::queries::{domain_events, order_payments, orders};

fn choice(method: OfflineMethod, change_for_centavos: Option<i64>) -> OfflinePaymentChoice {
    OfflinePaymentChoice {
        method,
        change_for_centavos,
    }
}

fn operation<'a>(payment: &'a OrderPayment, account_id: &'a str) -> PaymentOperation<'a> {
    PaymentOperation {
        establishment_id: &payment.establishment_id,
        order_id: &payment.order_id,
        payment_id: &payment.id,
        expected_version: payment.version,
        actor: Actor::Owner,
        actor_account_id: account_id,
        event_context: Default::default(),
    }
}

async fn setup(slug: &str) -> (support::Harness, String, Order, DateTime<Utc>) {
    let harness = support::harness(false).await;
    let (owner, establishment) = support::owner(&harness.pool, slug).await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let placement = fixture::catalog(&harness, &establishment, now).await;
    let order = order_creation::create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    (harness, owner.account.id, order, now)
}

#[tokio::test]
async fn completed_order_stays_unpaid_until_exact_collection_is_confirmed() {
    let (harness, owner, mut order, now) = setup("offline-completed-unpaid").await;
    let pool = &harness.pool;
    let est = order.establishment_id.clone();
    let pending = create_pending(
        pool,
        &est,
        &order.id,
        choice(OfflineMethod::Cash, Some(5_000)),
        now,
    )
    .await
    .unwrap();
    for action in [
        OrderAction::Accept {
            promised_at: now + Duration::minutes(35),
        },
        OrderAction::MarkReady,
        OrderAction::Complete,
    ] {
        let command = TransitionOrder {
            expected_state: order.state,
            expected_version: order.version,
            action,
            actor: Actor::Owner,
            actor_account_id: Some(&owner),
            event_context: Default::default(),
        };
        order = order_transitions::transition(pool, &est, &order.id, command, now)
            .await
            .unwrap();
    }
    assert_eq!(order.state, OrderState::Completed);
    assert_eq!(
        order_payments::current(pool, &est, &order.id)
            .await
            .unwrap(),
        Some(pending.clone())
    );
    let (first, second) = tokio::join!(
        confirm(
            pool,
            operation(&pending, &owner),
            OfflineMethod::CardOnDelivery,
            now
        ),
        confirm(
            pool,
            operation(&pending, &owner),
            OfflineMethod::CardOnDelivery,
            now
        ),
    );
    let collected = match (first, second) {
        (Ok(payment), Err(AppError::Conflict(_))) | (Err(AppError::Conflict(_)), Ok(payment)) => {
            payment
        }
        pair => panic!("expected one collection, got {pair:?}"),
    };
    assert_eq!(collected.requested_method, OfflineMethod::Cash);
    assert_eq!(collected.method, OfflineMethod::CardOnDelivery);
    assert_eq!(collected.state, OrderPaymentState::Confirmed);
    assert_eq!(collected.version, 2);
    assert_eq!(
        orders::get(pool, &est, &order.id).await.unwrap(),
        Some(order.clone())
    );
    let events = domain_events::list_for_establishment(pool, &est)
        .await
        .unwrap();
    let collections: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == "PaymentConfirmed")
        .collect();
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].aggregate_type, "order_payment");
    assert_eq!(collections[0].aggregate_id, collected.id);
    assert_eq!(
        serde_json::from_str::<Value>(&collections[0].payload).unwrap(),
        json!({
            "payment_id": collected.id,
            "order_id": order.id,
            "amount_centavos": order.total_centavos,
            "method": "card_on_delivery",
        })
    );
}

#[tokio::test]
async fn refunds_preserve_auditable_exact_balances_and_roll_back_if_event_is_unavailable() {
    let (harness, owner, order, now) = setup("offline-refund-invariants").await;
    let pool = &harness.pool;
    let est = &order.establishment_id;
    let pending = create_pending(
        pool,
        est,
        &order.id,
        choice(OfflineMethod::PixOnDelivery, None),
        now,
    )
    .await
    .unwrap();
    assert!(matches!(
        refund(
            pool,
            operation(&pending, &owner),
            1,
            RefundReason::Other,
            now
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    let collected = confirm(
        pool,
        operation(&pending, &owner),
        OfflineMethod::PixOnDelivery,
        now,
    )
    .await
    .unwrap();
    for invalid in [0, -1, i64::MAX, collected.amount_centavos + 1] {
        assert!(matches!(
            refund(
                pool,
                operation(&collected, &owner),
                invalid,
                RefundReason::Other,
                now
            )
            .await,
            Err(AppError::Validation("commerce.invalid_refund_amount"))
        ));
    }
    sqlx::query(
        "CREATE TRIGGER fail_test_payment_outbox BEFORE INSERT ON domain_events WHEN new.event_type = 'PaymentRefunded'
         BEGIN SELECT RAISE(ABORT, 'simulated outbox failure'); END",
    )
    .execute(pool).await.unwrap();
    assert!(matches!(
        refund(
            pool,
            operation(&collected, &owner),
            100,
            RefundReason::Other,
            now
        )
        .await,
        Err(AppError::Database(_))
    ));
    let recorded: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payment_refunds WHERE establishment_id = ?")
            .bind(est)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(recorded, 0);
    assert_eq!(
        order_payments::current(pool, est, &order.id).await.unwrap(),
        Some(collected.clone())
    );
    sqlx::query("DROP TRIGGER fail_test_payment_outbox")
        .execute(pool)
        .await
        .unwrap();

    let mut payment = collected;
    let allocations = [100, 150, order.total_centavos - 250];
    let mut total = 0;
    for (index, amount) in allocations.into_iter().enumerate() {
        let command = operation(&payment, &owner);
        let changed = refund(pool, command, amount, RefundReason::CustomerRequest, now)
            .await
            .unwrap();
        assert!(matches!(
            refund(pool, command, 1, RefundReason::CustomerRequest, now).await,
            Err(AppError::Conflict(_))
        ));
        total += amount;
        assert_eq!(changed.refunded_centavos, total);
        assert_eq!(changed.version, i64::try_from(index).unwrap() + 3);
        payment = changed;
    }
    assert_eq!(payment.state, OrderPaymentState::Refunded);
    let stored: Vec<(String, i64)> = sqlx::query_as(
        "SELECT id, amount_centavos FROM payment_refunds WHERE payment_id = ? AND establishment_id = ? ORDER BY id",
    )
    .bind(&payment.id).bind(est).fetch_all(pool).await.unwrap();
    assert_eq!(
        stored.iter().map(|row| row.1).sum::<i64>(),
        order.total_centavos
    );
    let events = domain_events::list_for_establishment(pool, est)
        .await
        .unwrap();
    let refunds: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == "PaymentRefunded")
        .collect();
    assert_eq!(refunds.len(), stored.len());
    for (event, (refund_id, amount)) in refunds.iter().zip(stored) {
        assert_eq!(
            serde_json::from_str::<Value>(&event.payload).unwrap(),
            json!({
                "payment_id": payment.id,
                "order_id": order.id,
                "amount_centavos": amount,
                "refund_id": refund_id,
            })
        );
    }
    assert!(
        sqlx::query("DELETE FROM order_payments WHERE id = ? AND establishment_id = ?")
            .bind(&payment.id)
            .bind(est)
            .execute(pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("UPDATE payment_refunds SET amount_centavos = 1 WHERE payment_id = ? AND establishment_id = ?")
        .bind(&payment.id).bind(est).execute(pool).await.is_err());
}
