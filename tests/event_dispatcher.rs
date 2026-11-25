#[path = "commercial/support.rs"]
mod support;

use std::future::pending;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio::sync::Notify;
use tower::ServiceExt;

use qrcardapio::application::auth;
use qrcardapio::domain::events::{EventType, NewEvent};
use qrcardapio::error::{AppError, AppResult};
use qrcardapio::infrastructure::database::begin_immediate;
use qrcardapio::infrastructure::database::queries::domain_events::{self, StoredEvent};
use qrcardapio::infrastructure::database::queries::{event_consumers, event_dispatch_status};
use qrcardapio::infrastructure::events::dispatcher::{Consumer, dispatch_batch};
use qrcardapio::infrastructure::security::PasswordService;

const MAIN: Consumer = Consumer {
    name: "test.orders",
    event_types: &[EventType::OrderCreated],
    handle: |tx, event| Box::pin(record(tx, event)),
};

async fn init() -> support::Harness {
    let harness = support::harness(false).await;
    sqlx::raw_sql(
        "INSERT INTO establishments (id, slug, name, created_at, updated_at)
         VALUES ('one', 'tenant-one', 'One', datetime('now'), datetime('now')),
                ('two', 'tenant-two', 'Two', datetime('now'), datetime('now'));
         CREATE TABLE consumer_effects (seq INTEGER NOT NULL);",
    )
    .execute(&harness.pool)
    .await
    .unwrap();
    harness
}

async fn append_events(pool: &SqlitePool, rows: &[(&str, &str, EventType, bool)]) {
    let mut tx = begin_immediate(pool).await.unwrap();
    for &(establishment, aggregate, kind, fail) in rows {
        let event = NewEvent::new(kind, establishment, aggregate, json!({ "fail": fail }));
        assert!(domain_events::append(&mut tx, &event).await.unwrap());
    }
    tx.commit().await.unwrap();
}

async fn record(tx: &mut Transaction<'_, Sqlite>, event: &StoredEvent) -> AppResult<()> {
    sqlx::query("INSERT INTO consumer_effects VALUES (?)")
        .bind(event.seq)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn fail_selected(tx: &mut Transaction<'_, Sqlite>, event: &StoredEvent) -> AppResult<()> {
    record(tx, event).await?;
    let payload: Value = serde_json::from_str(&event.payload).unwrap();
    if payload["fail"] == true {
        Err(AppError::Internal(
            "handler failure details stay private".to_string(),
        ))
    } else {
        Ok(())
    }
}

async fn effects(pool: &SqlitePool) -> Vec<i64> {
    sqlx::query_scalar("SELECT seq FROM consumer_effects ORDER BY rowid")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn unpublished(pool: &SqlitePool) -> Vec<i64> {
    sqlx::query_scalar("SELECT event_seq FROM event_dispatch_publications WHERE published_at IS NULL ORDER BY event_seq")
        .fetch_all(pool)
        .await
        .unwrap()
}

static MID_BATCH: Notify = Notify::const_new();

#[tokio::test]
async fn interrupted_batch_rolls_back_effects_receipts_and_checkpoint_before_replay() {
    let harness = init().await;
    let pool = &harness.pool;
    append_events(
        pool,
        &[
            ("one", "first", EventType::OrderCreated, false),
            ("one", "second", EventType::OrderCreated, false),
        ],
    )
    .await;
    let interrupted = Consumer {
        handle: |tx, event| {
            Box::pin(async move {
                record(tx, event).await?;
                if event.aggregate_id == "second" {
                    MID_BATCH.notify_one();
                    pending::<()>().await;
                }
                Ok(())
            })
        },
        ..MAIN
    };
    let worker_pool = pool.clone();
    let worker =
        tokio::spawn(async move { dispatch_batch(&worker_pool, &[interrupted], Utc::now()).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), MID_BATCH.notified())
        .await
        .unwrap();
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    assert!(effects(pool).await.is_empty());
    assert_eq!(
        event_consumers::checkpoint(pool, MAIN.name).await.unwrap(),
        None
    );
    let ready = Utc::now();
    let workers = [MAIN];
    let (recovery, competing) = tokio::join!(
        dispatch_batch(pool, &workers, ready),
        dispatch_batch(pool, &workers, ready)
    );
    recovery.unwrap();
    competing.unwrap();
    assert_eq!(effects(pool).await, vec![1, 2]);
    let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_consumer_receipts")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(receipts, 2);
    assert_eq!(
        event_consumers::checkpoint(pool, MAIN.name).await.unwrap(),
        Some(2)
    );
    assert!(unpublished(pool).await.is_empty());
}

#[tokio::test]
async fn backoff_parks_only_the_same_tenant_aggregate_then_warns_on_dead_letter() {
    let harness = init().await;
    let pool = &harness.pool;
    let failing = Consumer {
        handle: |tx, event| Box::pin(fail_selected(tx, event)),
        ..MAIN
    };
    append_events(
        pool,
        &[
            ("one", "shared", EventType::OrderCreated, true),
            ("two", "shared", EventType::OrderCreated, false),
            ("one", "shared", EventType::OrderCreated, false),
            ("one", "independent", EventType::OrderCreated, false),
            ("one", "shared", EventType::OrderAccepted, false),
        ],
    )
    .await;
    let now = Utc::now();
    dispatch_batch(pool, &[failing], now).await.unwrap();
    assert_eq!(effects(pool).await, vec![2, 4]);
    assert_eq!(unpublished(pool).await, vec![1, 3]);
    let initial = event_dispatch_status::status(pool, now).await.unwrap();
    assert_eq!(initial.pending.pending_events, 2);
    assert_eq!(initial.consumers[0].retry_count, 1);
    assert_eq!(initial.consumers[0].parked_count, 1);
    assert_eq!(initial.consumers[0].last_seq, 5);
    assert_eq!(dispatch_batch(pool, &[failing], now).await.unwrap(), 0);

    for attempt in 1..10 {
        let (attempts, retry_at): (i64, i64) = sqlx::query_as(
            "SELECT attempts, next_attempt_at FROM event_consumer_failures WHERE event_seq = 1",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(attempts, attempt);
        if attempt == 1 {
            assert!((1000..=1250).contains(&(retry_at - now.timestamp_millis())));
        }
        dispatch_batch(
            pool,
            &[failing],
            DateTime::from_timestamp_millis(retry_at).unwrap(),
        )
        .await
        .unwrap();
    }
    append_events(pool, &[("two", "shared", EventType::OrderCreated, false)]).await;
    dispatch_batch(pool, &[failing], now).await.unwrap();
    assert_eq!(effects(pool).await, vec![2, 4, 6]);
    assert_eq!(unpublished(pool).await, vec![1, 3]);
    let status = event_dispatch_status::status(pool, now + Duration::minutes(2))
        .await
        .unwrap();
    assert_eq!(status.dead_letters[0].attempts, 10);
    assert_eq!(status.dead_letters[0].error_key, "error.internal");
    assert_eq!(status.consumers[0].dead_letter_count, 1);
    assert!(event_dispatch_status::lag_warning(pool, now).await.unwrap());

    let readiness = harness
        .app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(readiness.status(), StatusCode::OK);
    let readiness: Value =
        serde_json::from_slice(&readiness.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(readiness["warnings"], json!(["outbox_lag"]));
    assert_eq!(readiness.as_object().unwrap().len(), 4);

    let (owner, _) = support::owner(pool, "outbox-operator").await;
    assert_eq!(
        support::get(&harness.app, &owner, "/admin/outbox")
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    support::admin(pool, "admin-outbox@example.test").await;
    let admin = auth::login(
        pool,
        &PasswordService::new(),
        "admin-outbox@example.test",
        "AdminPassword123",
    )
    .await
    .unwrap();
    let diagnostics = support::get(&harness.app, &admin, "/admin/outbox").await;
    assert_eq!(diagnostics.status(), StatusCode::OK);
    assert_eq!(diagnostics.headers()["cache-control"], "no-store");
    let body = diagnostics.into_body().collect().await.unwrap().to_bytes();
    let diagnostics: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(diagnostics["dead_letters"][0]["seq"], 1);
    assert!(diagnostics["dead_letters"][0].get("payload").is_none());
}

#[tokio::test]
async fn publication_waits_for_every_subscriber_and_parked_events_resume_in_order() {
    let harness = init().await;
    let pool = &harness.pool;
    append_events(
        pool,
        &[
            ("one", "shared", EventType::OrderCreated, true),
            ("one", "shared", EventType::OrderCreated, false),
        ],
    )
    .await;
    let received = Consumer {
        name: "test.acknowledge",
        handle: |_, _| Box::pin(async { Ok(()) }),
        ..MAIN
    };
    let failing = Consumer {
        handle: |tx, event| Box::pin(fail_selected(tx, event)),
        ..MAIN
    };
    let now = Utc::now();
    dispatch_batch(pool, &[received, failing], now)
        .await
        .unwrap();
    assert_eq!(unpublished(pool).await, vec![1, 2]);
    let later = now + Duration::seconds(2);
    dispatch_batch(pool, &[received, MAIN], later)
        .await
        .unwrap();
    assert_eq!(effects(pool).await, vec![1]);
    assert_eq!(unpublished(pool).await, vec![2]);
    dispatch_batch(pool, &[received, MAIN], later)
        .await
        .unwrap();
    assert_eq!(effects(pool).await, vec![1, 2]);
    assert!(unpublished(pool).await.is_empty());
}
