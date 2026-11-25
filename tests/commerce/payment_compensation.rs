use super::*;

use qrcardapio::application::commerce::payments::compensate_cancelled_order_in;
use qrcardapio::domain::commerce::order::{CancelReason, RejectReason};
use qrcardapio::infrastructure::database::begin_immediate;
use qrcardapio::infrastructure::database::queries::domain_events::StoredEvent;

async fn cancel_order(
    pool: &sqlx::SqlitePool,
    order: &Order,
    owner: &str,
    action: OrderAction,
    now: DateTime<Utc>,
) -> StoredEvent {
    let (actor, actor_account_id) = if matches!(action, OrderAction::Reject { .. }) {
        (Actor::Owner, Some(owner))
    } else {
        (Actor::Consumer, None)
    };
    let placed = TransitionOrder {
        expected_state: order.state,
        expected_version: order.version,
        action,
        actor,
        actor_account_id,
        event_context: Default::default(),
    };
    order_transitions::transition(pool, &order.establishment_id, &order.id, placed, now)
        .await
        .unwrap();
    domain_events::list_for_establishment(pool, &order.establishment_id)
        .await
        .unwrap()
        .pop()
        .unwrap()
}

#[tokio::test]
async fn rejected_order_prevents_confirmation_before_dispatch_and_cancels_pending_once() {
    let (harness, owner, order, now) = setup("offline-order-rejected").await;
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
    let event = cancel_order(
        pool,
        &order,
        &owner,
        OrderAction::Reject {
            reason: RejectReason::TooBusy,
        },
        now,
    )
    .await;
    assert_eq!(event.event_type, "OrderRejected");
    assert!(matches!(
        confirm(
            pool,
            operation(&pending, &owner),
            OfflineMethod::PixOnDelivery,
            now
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        order_payments::current(pool, est, &order.id).await.unwrap(),
        Some(pending.clone())
    );
    // Simulates failure after effects and before the dispatcher records a receipt.
    {
        let mut tx = begin_immediate(pool).await.unwrap();
        compensate_cancelled_order_in(&mut tx, &event)
            .await
            .unwrap();
        assert!(
            order_payments::current(&mut *tx, est, &order.id)
                .await
                .unwrap()
                .is_none()
        );
        tx.rollback().await.unwrap();
    }
    assert_eq!(
        order_payments::current(pool, est, &order.id).await.unwrap(),
        Some(pending.clone())
    );

    for _ in 0..2 {
        let mut tx = begin_immediate(pool).await.unwrap();
        compensate_cancelled_order_in(&mut tx, &event)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let cancelled = order_payments::get(pool, est, &order.id, &pending.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.state, OrderPaymentState::Cancelled);
    assert_eq!(cancelled.version, pending.version + 1);
    let traces: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT actor_kind, actor_account_id, causation_id FROM order_payment_transitions
         WHERE payment_id = ? AND establishment_id = ?",
    )
    .bind(&pending.id)
    .bind(est)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(traces, [("system".into(), None, event.event_id)]);
    assert_eq!(
        domain_events::list_for_establishment(pool, est)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        create_pending(pool, est, &order.id, choice(OfflineMethod::Cash, None), now).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn cancelled_order_records_only_remaining_refund_due_until_staff_confirms_return() {
    let (harness, owner, order, now) = setup("offline-order-cancelled").await;
    let pool = &harness.pool;
    let est = &order.establishment_id;
    let pending = create_pending(pool, est, &order.id, choice(OfflineMethod::Cash, None), now)
        .await
        .unwrap();
    let collected = confirm(pool, operation(&pending, &owner), OfflineMethod::Cash, now)
        .await
        .unwrap();
    let part_refunded = refund(
        pool,
        operation(&collected, &owner),
        250,
        RefundReason::PaymentError,
        now,
    )
    .await
    .unwrap();
    let event = cancel_order(
        pool,
        &order,
        &owner,
        OrderAction::Cancel {
            reason: CancelReason::ConsumerRequest,
        },
        now,
    )
    .await;
    assert_eq!(event.event_type, "OrderCancelled");
    let mut tx = begin_immediate(pool).await.unwrap();
    compensate_cancelled_order_in(&mut tx, &event)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let marked = order_payments::current(pool, est, &order.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(marked.state, OrderPaymentState::PartiallyRefunded);
    assert_eq!(marked.version, part_refunded.version);
    assert_eq!(marked.refunded_centavos, 250);
    assert_eq!(marked.refund_due_centavos, collected.amount_centavos - 250);
    assert_eq!(
        marked.refund_due_event_id.as_deref(),
        Some(event.event_id.as_str())
    );
    let compensation_refunds: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_refunds WHERE payment_id = ? AND establishment_id = ?",
    )
    .bind(&marked.id)
    .bind(est)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        compensation_refunds, 1,
        "only the recorded staff refund exists"
    );

    let partial = refund(
        pool,
        operation(&marked, &owner),
        125,
        RefundReason::OrderCancelled,
        now,
    )
    .await
    .unwrap();
    assert_eq!(partial.refund_due_centavos, collected.amount_centavos - 375);
    let mut tx = begin_immediate(pool).await.unwrap();
    compensate_cancelled_order_in(&mut tx, &event)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        order_payments::current(pool, est, &order.id).await.unwrap(),
        Some(partial.clone())
    );
    let settled = refund(
        pool,
        operation(&partial, &owner),
        partial.refund_due_centavos,
        RefundReason::OrderCancelled,
        now,
    )
    .await
    .unwrap();
    assert_eq!(settled.state, OrderPaymentState::Refunded);
    assert_eq!(settled.refund_due_centavos, 0);
    assert_eq!(settled.refunded_centavos, collected.amount_centavos);
    assert_eq!(settled.refund_due_event_id, Some(event.event_id));
}
