#[path = "commerce/order_fixtures.rs"]
mod fixture;
#[path = "commercial/support.rs"]
mod support;

use chrono::Duration;
use serde_json::{Value, json};

use qrcardapio::application::commerce::{order_creation::create, order_transitions::transition};
use qrcardapio::domain::commerce::order::{
    CancelReason, Order, OrderAction, RejectReason, TransitionOrder,
};
use qrcardapio::domain::commerce::state_machine::{Actor, OrderState};
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::queries::{domain_events, orders};

#[tokio::test]
async fn acceptance_timer_rejects_once_and_dispatcher_cancels_only_uncollected_payment() {
    use qrcardapio::application::commerce::{order_expiry, payments, worker};
    use qrcardapio::domain::commerce::order_payment::{OfflineMethod, OfflinePaymentChoice};
    use qrcardapio::domain::commerce::state_machine::OrderPaymentState;
    use qrcardapio::infrastructure::database::queries::order_payments;
    use qrcardapio::infrastructure::events::dispatcher::dispatch_batch;

    let harness = support::harness(false).await;
    let (owner, establishment) = support::owner(&harness.pool, "order-expiry").await;
    let now = fixture::at("2026-11-25T15:00:00.123Z");
    let placement = fixture::catalog(&harness, &establishment, now).await;
    let expiring = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    let accepted = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    let payment = payments::create_pending(
        &harness.pool,
        &establishment,
        &expiring.id,
        OfflinePaymentChoice {
            method: OfflineMethod::Cash,
            change_for_centavos: None,
        },
        now,
    )
    .await
    .unwrap();
    let expiration = expiring.acceptance_deadline_at;
    transition(
        &harness.pool,
        &establishment,
        &accepted.id,
        owner_transition(
            &accepted,
            &owner.account.id,
            OrderAction::Accept {
                promised_at: expiration + Duration::minutes(20),
            },
        ),
        now,
    )
    .await
    .unwrap();
    assert_eq!(
        order_expiry::expire_due(&harness.pool, expiration - Duration::milliseconds(1))
            .await
            .unwrap(),
        0
    );
    let (first, second) = tokio::join!(
        order_expiry::expire_due(&harness.pool, expiration),
        order_expiry::expire_due(&harness.pool, expiration)
    );
    assert_eq!(first.unwrap() + second.unwrap(), 1);
    for _ in 0..2 {
        dispatch_batch(&harness.pool, worker::CONSUMERS, expiration)
            .await
            .unwrap();
    }
    let expiring = orders::get(&harness.pool, &establishment, &expiring.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(expiring.state, OrderState::Rejected);
    assert_eq!(expiring.reason_code.as_deref(), Some("acceptance_timeout"));
    let accepted = orders::get(&harness.pool, &establishment, &accepted.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(accepted.state, OrderState::Accepted);
    let payment = order_payments::get(&harness.pool, &establishment, &expiring.id, &payment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(payment.state, OrderPaymentState::Cancelled);
    assert_eq!(payment.version, 2);
    let events = domain_events::list_for_establishment(&harness.pool, &establishment)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "OrderRejected")
            .count(),
        1
    );
}

fn owner_transition<'a>(
    order: &Order,
    account: &'a str,
    action: OrderAction,
) -> TransitionOrder<'a> {
    TransitionOrder {
        expected_state: order.state,
        expected_version: order.version,
        action,
        actor: Actor::Owner,
        actor_account_id: Some(account),
        event_context: Default::default(),
    }
}

#[tokio::test]
async fn concurrent_order_transitions_commit_one_event_and_return_one_conflict() {
    let harness = support::harness(false).await;
    let (owner, establishment) = support::owner(&harness.pool, "order-race").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let placement = fixture::catalog(&harness, &establishment, now).await;
    let order = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    let promised_at = now + Duration::minutes(45);
    let command = owner_transition(
        &order,
        &owner.account.id,
        OrderAction::Accept { promised_at },
    );

    let (x, y) = tokio::join!(
        transition(&harness.pool, &establishment, &order.id, command, now),
        transition(&harness.pool, &establishment, &order.id, command, now)
    );
    let (changed, error) = match (x, y) {
        (Ok(order), Err(error)) | (Err(error), Ok(order)) => (order, error),
        pair => panic!("expected one commit and one conflict, got {pair:?}"),
    };
    assert!(matches!(error, AppError::Conflict(_)));
    assert_eq!(error.status_code(), 409);
    assert_eq!(changed.state, OrderState::Accepted);
    assert_eq!(changed.version, 2);
    assert_eq!(changed.accepted_at, Some(now));
    assert_eq!(changed.promised_at, Some(promised_at));
    assert_eq!(
        orders::get(&harness.pool, &establishment, &order.id)
            .await
            .unwrap(),
        Some(changed)
    );
    let log: Vec<(String, String, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT from_state, to_state, actor_kind, actor_account_id, version
         FROM order_transitions WHERE order_id = ? AND establishment_id = ?",
    )
    .bind(&order.id)
    .bind(&establishment)
    .fetch_all(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        log,
        [(
            "placed".into(),
            "accepted".into(),
            "owner".into(),
            Some(owner.account.id),
            2
        )]
    );
    for sql in [
        "UPDATE order_transitions SET reason_code = 'changed' WHERE establishment_id = ?",
        "DELETE FROM order_transitions WHERE establishment_id = ?",
    ] {
        assert!(
            sqlx::query(sql)
                .bind(&establishment)
                .execute(&harness.pool)
                .await
                .is_err()
        );
    }
    let events = domain_events::list_for_establishment(&harness.pool, &establishment)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].event_type, "OrderAccepted");
    assert_eq!(
        serde_json::from_str::<Value>(&events[1].payload).unwrap(),
        json!({ "order_id": order.id, "promised_at": promised_at.to_rfc3339() })
    );
}

#[tokio::test]
async fn completing_a_delivery_order_emits_order_facts_without_settling_payment() {
    let harness = support::harness(false).await;
    let (owner, establishment) = support::owner(&harness.pool, "order-delivered").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let placement = fixture::catalog(&harness, &establishment, now).await;
    let mut order = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    let promised_at = now + Duration::minutes(40);
    let delivered_at = now + Duration::minutes(30);
    for (action, at) in [
        (OrderAction::Accept { promised_at }, now),
        (OrderAction::Prepare, now + Duration::minutes(10)),
        (OrderAction::MarkReady, now + Duration::minutes(20)),
        (OrderAction::Complete, delivered_at),
    ] {
        let command = owner_transition(&order, &owner.account.id, action);
        order = transition(&harness.pool, &establishment, &order.id, command, at)
            .await
            .unwrap();
    }
    assert_eq!(order.completed_at, Some(delivered_at));
    assert_eq!(order.state, OrderState::Completed);
    assert_eq!(order.version, 5);
    assert!(matches!(
        transition(
            &harness.pool,
            &establishment,
            &order.id,
            owner_transition(
                &order,
                &owner.account.id,
                OrderAction::Cancel {
                    reason: CancelReason::Other
                }
            ),
            delivered_at,
        )
        .await,
        Err(AppError::Conflict(_))
    ));

    let events = domain_events::list_for_establishment(&harness.pool, &establishment)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        [
            "OrderCreated",
            "OrderAccepted",
            "OrderPreparing",
            "OrderReady",
            "OrderCompleted"
        ]
    );
    for (event, expected) in events.iter().skip(2).zip([
        json!({ "order_id": order.id }),
        json!({ "order_id": order.id, "fulfillment": "delivery" }),
        json!({ "order_id": order.id, "completed_at": delivered_at.to_rfc3339() }),
    ]) {
        assert_eq!(event.aggregate_type, "order");
        assert_eq!(
            serde_json::from_str::<Value>(&event.payload).unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn rejection_and_cancellation_publish_only_registered_reason_codes() {
    let harness = support::harness(false).await;
    let (_, establishment) = support::owner(&harness.pool, "order-reasons").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let placement = fixture::catalog(&harness, &establishment, now).await;
    let expired = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    let cancelled = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    let rejection = TransitionOrder {
        expected_state: OrderState::Placed,
        expected_version: 1,
        action: OrderAction::Reject {
            reason: RejectReason::AcceptanceTimeout,
        },
        actor: Actor::System,
        actor_account_id: None,
        event_context: Default::default(),
    };
    assert!(matches!(
        transition(&harness.pool, &establishment, &expired.id, rejection, now).await,
        Err(AppError::Conflict(_))
    ));
    let rejected = transition(
        &harness.pool,
        &establishment,
        &expired.id,
        rejection,
        expired.acceptance_deadline_at,
    )
    .await
    .unwrap();
    assert_eq!(rejected.state, OrderState::Rejected);
    assert_eq!(rejected.rejected_at, Some(expired.acceptance_deadline_at));

    let consumer = TransitionOrder {
        action: OrderAction::Cancel {
            reason: CancelReason::ConsumerRequest,
        },
        actor: Actor::Consumer,
        ..rejection
    };
    let result = transition(&harness.pool, &establishment, &cancelled.id, consumer, now)
        .await
        .unwrap();
    assert_eq!(result.state, OrderState::Cancelled);
    assert_eq!(result.cancelled_at, Some(now));

    let events = domain_events::list_for_establishment(&harness.pool, &establishment)
        .await
        .unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(events[2].event_type, "OrderRejected");
    assert_eq!(
        serde_json::from_str::<Value>(&events[2].payload).unwrap(),
        json!({ "order_id": expired.id, "reason_code": "acceptance_timeout" })
    );
    assert_eq!(events[3].event_type, "OrderCancelled");
    assert_eq!(
        serde_json::from_str::<Value>(&events[3].payload).unwrap(),
        json!({ "order_id": cancelled.id, "reason_code": "consumer_request", "cancelled_by": "consumer" })
    );
}

#[tokio::test]
async fn order_creation_and_transition_roll_back_with_outbox_and_enforce_tenant() {
    let harness = support::harness(false).await;
    let (owner, establishment) = support::owner(&harness.pool, "order-rollback").await;
    let (intruder, other) = support::owner(&harness.pool, "order-intruder").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let placement = fixture::catalog(&harness, &establishment, now).await;

    sqlx::query(
        "CREATE TRIGGER fail_order_created_outbox BEFORE INSERT ON domain_events
         WHEN new.event_type = 'OrderCreated' BEGIN SELECT RAISE(ABORT, 'test failure'); END",
    )
    .execute(&harness.pool)
    .await
    .unwrap();
    assert!(matches!(
        create(&harness.pool, &establishment, &placement, now).await,
        Err(AppError::Database(_))
    ));
    let created_rows: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM orders WHERE establishment_id = ?1)
                + (SELECT COUNT(*) FROM order_items WHERE establishment_id = ?1)
                + (SELECT COUNT(*) FROM order_item_options WHERE establishment_id = ?1)",
    )
    .bind(&establishment)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(created_rows, 0);
    sqlx::query("DROP TRIGGER fail_order_created_outbox")
        .execute(&harness.pool)
        .await
        .unwrap();
    let order = create(&harness.pool, &establishment, &placement, now)
        .await
        .unwrap();
    assert_eq!(order.order_number, 1);

    let accept = OrderAction::Accept {
        promised_at: now + Duration::minutes(40),
    };
    assert!(matches!(
        transition(
            &harness.pool,
            &other,
            &order.id,
            owner_transition(&order, &intruder.account.id, accept),
            now,
        )
        .await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        transition(
            &harness.pool,
            &establishment,
            &order.id,
            owner_transition(&order, &intruder.account.id, accept),
            now,
        )
        .await,
        Err(AppError::Forbidden(_))
    ));

    sqlx::query(
        "CREATE TRIGGER fail_order_accepted_outbox BEFORE INSERT ON domain_events
         WHEN new.event_type = 'OrderAccepted' BEGIN SELECT RAISE(ABORT, 'test failure'); END",
    )
    .execute(&harness.pool)
    .await
    .unwrap();
    assert!(matches!(
        transition(
            &harness.pool,
            &establishment,
            &order.id,
            owner_transition(&order, &owner.account.id, accept),
            now,
        )
        .await,
        Err(AppError::Database(_))
    ));
    assert_eq!(
        orders::get(&harness.pool, &establishment, &order.id)
            .await
            .unwrap(),
        Some(order.clone())
    );
    let transitions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM order_transitions WHERE order_id = ? AND establishment_id = ?",
    )
    .bind(&order.id)
    .bind(&establishment)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(transitions, 0);
    assert_eq!(
        domain_events::list_for_establishment(&harness.pool, &establishment)
            .await
            .unwrap()
            .len(),
        1
    );
}
