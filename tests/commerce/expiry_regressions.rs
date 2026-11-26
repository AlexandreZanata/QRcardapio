use chrono::Duration;

use qrcardapio::application::commerce::{order_creation::create, order_expiry::expire_due};
use qrcardapio::domain::commerce::state_machine::OrderState;
use qrcardapio::infrastructure::database::queries::{domain_events, orders};

use crate::{fixture, support};

#[tokio::test]
async fn failed_expiry_batch_preserves_orders_and_does_not_block_other_tenants() {
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let (_, failing_tenant) = support::owner(pool, "expiry-failing").await;
    let (_, healthy_tenant) = support::owner(pool, "expiry-healthy").await;
    let now = fixture::at("2026-11-26T15:00:00.123Z");
    let failing_catalog = fixture::catalog(&harness, &failing_tenant, now).await;
    let failing = create(pool, &failing_tenant, &failing_catalog, now)
        .await
        .unwrap();
    for _ in 1..32 {
        create(pool, &failing_tenant, &failing_catalog, now)
            .await
            .unwrap();
    }
    let later = now + Duration::milliseconds(1);
    let healthy_catalog = fixture::catalog(&harness, &healthy_tenant, later).await;
    let healthy = create(pool, &healthy_tenant, &healthy_catalog, later)
        .await
        .unwrap();
    let due = failing.acceptance_deadline_at;

    sqlx::query(
        "CREATE TRIGGER fail_tenant_rejection BEFORE INSERT ON domain_events
         WHEN NEW.event_type = 'OrderRejected' AND EXISTS (
             SELECT 1 FROM establishments
             WHERE id = NEW.establishment_id AND slug = 'expiry-failing'
         ) BEGIN SELECT RAISE(ABORT, 'test failure'); END",
    )
    .execute(pool)
    .await
    .unwrap();

    assert_eq!(expire_due(pool, due).await.unwrap(), 0);
    let first_retry = due + Duration::seconds(30);
    // The valid order was due before those retries, even though its deadline is newer.
    assert_eq!(expire_due(pool, first_retry).await.unwrap(), 1);
    assert_eq!(expire_due(pool, first_retry).await.unwrap(), 0);
    assert_eq!(
        orders::get(pool, &healthy_tenant, &healthy.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        OrderState::Rejected
    );
    assert_eq!(
        orders::get(pool, &failing_tenant, &failing.id)
            .await
            .unwrap(),
        Some(failing.clone())
    );
    let (retries, transitions): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM order_expiry_retries WHERE establishment_id = ?1),
                (SELECT count(*) FROM order_transitions WHERE establishment_id = ?1)",
    )
    .bind(&failing_tenant)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!((retries, transitions), (32, 0));
    assert!(
        domain_events::list_for_establishment(pool, &failing_tenant)
            .await
            .unwrap()
            .iter()
            .all(|event| event.event_type != "OrderRejected")
    );

    sqlx::query("DROP TRIGGER fail_tenant_rejection")
        .execute(pool)
        .await
        .unwrap();
    let retry = first_retry + Duration::seconds(30);
    assert_eq!(
        expire_due(pool, retry - Duration::milliseconds(1))
            .await
            .unwrap(),
        0
    );
    assert_eq!(expire_due(pool, retry).await.unwrap(), 32);
    assert_eq!(expire_due(pool, retry).await.unwrap(), 0);
    let (retries, transitions): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM order_expiry_retries WHERE establishment_id = ?1),
                (SELECT count(*) FROM order_transitions WHERE establishment_id = ?1)",
    )
    .bind(&failing_tenant)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!((retries, transitions), (0, 32));
    assert_eq!(
        domain_events::list_for_establishment(pool, &failing_tenant)
            .await
            .unwrap()
            .iter()
            .filter(|event| event.event_type == "OrderRejected")
            .count(),
        32
    );
}
