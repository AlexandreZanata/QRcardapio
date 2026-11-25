//! Run pinned to one allowed CPU, e.g. `taskset -c 2 cargo test --release --locked
//! --test commerce_dispatch_benchmark -- --ignored --nocapture`.

#[path = "commercial/support.rs"]
mod support;

use std::time::{Duration, Instant};

use chrono::Utc;
use qrcardapio::domain::events::EventType;
use qrcardapio::infrastructure::events::dispatcher::{Consumer, dispatch_batch};
use sqlx::SqlitePool;
use tokio::time::{MissedTickBehavior, interval};

const BACKLOG: i64 = 100_000;
const BURST: i64 = 20;
const BURSTS: i64 = 100;
const CONSUMERS: [Consumer; 2] = [
    Consumer {
        name: "benchmark.first",
        event_types: &[EventType::OrderCreated],
        handle: |_, _| Box::pin(async { Ok(()) }),
    },
    Consumer {
        name: "benchmark.second",
        event_types: &[EventType::OrderCreated],
        handle: |_, _| Box::pin(async { Ok(()) }),
    },
];

async fn append(pool: &SqlitePool, first: i64, count: i64) {
    sqlx::query(
        "WITH RECURSIVE numbers(n) AS (
             SELECT ?1 UNION ALL SELECT n + 1 FROM numbers WHERE n + 1 < ?1 + ?2
         )
         INSERT INTO domain_events (
             event_id, event_type, event_version, establishment_id, aggregate_type, aggregate_id,
             correlation_id, occurred_at, payload
         )
         SELECT printf('bench-event-%d', n), 'OrderCreated', 1, 'dispatcher-benchmark', 'order',
                printf('bench-order-%d', n % 1000), printf('bench-event-%d', n),
                strftime('%Y-%m-%dT%H:%M:%fZ','now'), '{}'
         FROM numbers",
    )
    .bind(first)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn drain(pool: &SqlitePool) {
    while dispatch_batch(pool, &CONSUMERS, Utc::now()).await.unwrap() > 0 {
        tokio::task::yield_now().await;
    }
}

async fn published(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(published_at) FROM event_dispatch_publications")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "measures sustained database work on one CPU"]
async fn drain_and_steady_state() {
    let backlog = std::env::var("COMMERCE_OUTBOX_BACKLOG")
        .ok()
        .map_or(BACKLOG, |value| value.parse::<i64>().unwrap());
    assert!(backlog > 0);
    let harness = support::harness(false).await;
    let pool = &harness.pool;
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(synchronous, 2, "benchmark requires SQLite synchronous=FULL");
    sqlx::query(
        "INSERT INTO establishments (id, slug, name, created_at, updated_at)
         VALUES ('dispatcher-benchmark', 'dispatcher-benchmark', 'Benchmark', datetime('now'), datetime('now'))",
    )
    .execute(pool)
    .await
    .unwrap();
    append(pool, 1, backlog).await;
    let started = Instant::now();
    drain(pool).await;
    let duration = started.elapsed().as_secs_f64();
    assert_eq!(published(pool).await, backlog);
    println!(
        "dispatcher backlog: {backlog} events, {} consumers, {duration:.3} s, {:.0} events/s",
        CONSUMERS.len(),
        backlog as f64 / duration,
    );

    let expected = backlog + BURST * BURSTS;
    let producer = async {
        let mut pace = interval(Duration::from_millis(100));
        pace.set_missed_tick_behavior(MissedTickBehavior::Delay);
        for batch in 0..BURSTS {
            pace.tick().await;
            append(pool, backlog + 1 + batch * BURST, BURST).await;
        }
    };
    let dispatcher = async {
        let mut poll = interval(Duration::from_millis(250));
        poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
        while published(pool).await < expected {
            poll.tick().await;
            drain(pool).await;
        }
    };
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(120), async {
        tokio::join!(producer, dispatcher);
    })
    .await
    .expect("steady-state producer/dispatcher must finish");
    let duration = started.elapsed().as_secs_f64();
    let samples: Vec<i64> = sqlx::query_scalar(
        "SELECT p.published_at - CAST(unixepoch(e.occurred_at, 'subsec') * 1000 AS INTEGER)
         FROM event_dispatch_publications p JOIN domain_events e ON e.seq = p.event_seq
         WHERE p.event_seq > ? ORDER BY 1",
    )
    .bind(backlog)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(samples.len(), (BURST * BURSTS) as usize);
    assert!(samples[0] >= 0);
    let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_consumer_receipts")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(receipts, expected * CONSUMERS.len() as i64);
    println!(
        "dispatcher steady: {} events, {duration:.3} s, {:.0} events/s, lag p95={} ms, max={} ms",
        samples.len(),
        samples.len() as f64 / duration,
        samples[(samples.len() * 95).div_ceil(100) - 1],
        samples.last().unwrap(),
    );
}
