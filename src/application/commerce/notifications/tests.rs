use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::TempDir;

use super::{process_batch, register, revoke};
use crate::application::commerce::worker::CONSUMERS;
use crate::config::DbSynchronous;
use crate::domain::events::{EventType, NewEvent};
use crate::infrastructure::database::queries::{commerce_notifications, domain_events};
use crate::infrastructure::database::{MIGRATOR, begin_immediate, open_pool_at};
use crate::infrastructure::events::dispatcher::dispatch_batch;
use crate::infrastructure::web_push::{DeliveryResult, PushSubscription};

#[path = "benchmark.rs"]
mod benchmark;

async fn setup() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_pool_at(&dir.path().join("push.sqlite"), 4, DbSynchronous::Full)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO establishments (id, slug, name, created_at, updated_at) VALUES
            ('one', 'one', 'One', '2026-11-25', '2026-11-25'),
            ('two', 'two', 'Two', '2026-11-25', '2026-11-25');
         INSERT INTO accounts (id, email, password_hash, preferred_locale, created_at, updated_at) VALUES
            ('staff', 'staff@example.test', 'fixture', 'es', '2026-11-25', '2026-11-25'),
            ('other', 'other@example.test', 'fixture', 'en', '2026-11-25', '2026-11-25');
         INSERT INTO establishment_members (establishment_id, account_id, created_at) VALUES
            ('one', 'staff', '2026-11-25'), ('two', 'other', '2026-11-25');
         INSERT INTO catalog_snapshots (establishment_id, channel, version, content_hash,
            snapshot_json, published_at, is_current)
         VALUES ('one', 'pickup', 1, hex(zeroblob(32)), '{}', '2026-11-25', 1);",
    ).execute(&pool).await.unwrap();
    (dir, pool)
}

fn subscription(index: usize) -> PushSubscription {
    PushSubscription::parse(
        &format!("https://fcm.googleapis.com/fcm/send/fake-{index}"),
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        "BTBZMqHH6r4Tts7J_aSIgg",
    )
    .unwrap()
}

async fn publish_order(pool: &SqlitePool, now: chrono::DateTime<Utc>) {
    let mut tx = begin_immediate(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO orders (id, establishment_id, public_id, order_number, business_date,
            channel, fulfillment, catalog_channel, catalog_version, items_subtotal_centavos,
            discount_centavos, delivery_fee_centavos, total_centavos, placed_at,
            acceptance_deadline_at, updated_at)
         VALUES ('order', 'one', '0123456789abcdef0123456789abcdef', 1, '2026-11-25',
            'pickup', 'pickup', 'pickup', 1, 1000, 0, 0, 1000, ?1, ?2, ?1)",
    )
    .bind(now)
    .bind(now + chrono::Duration::minutes(30))
    .execute(&mut *tx)
    .await
    .unwrap();
    let event = NewEvent::new(
        EventType::OrderCreated,
        "one",
        "order",
        json!({
            "order_id": "order", "channel": "pickup", "total_centavos": 1000,
            "item_count": 1, "catalog_version": 1,
        }),
    );
    assert!(domain_events::append(&mut tx, &event).await.unwrap());
    tx.commit().await.unwrap();
    while dispatch_batch(pool, CONSUMERS, now).await.unwrap() != 0 {}
}

#[tokio::test]
async fn limits_notifications_to_opted_in_members_and_reclaims_interrupted_sends() {
    let (_dir, pool) = setup().await;
    let now = Utc::now();
    let first = register(&pool, "one", "staff", &subscription(1), now)
        .await
        .unwrap();
    register(&pool, "two", "other", &subscription(2), now)
        .await
        .unwrap();
    assert!(
        register(&pool, "two", "other", &subscription(1), now)
            .await
            .is_err()
    );
    assert!(
        register(&pool, "one", "other", &subscription(3), now)
            .await
            .is_err()
    );
    assert!(!revoke(&pool, "two", "other", &first).await.unwrap());
    publish_order(&pool, now).await;
    let now = Utc::now();

    let first_attempt = commerce_notifications::claim(&pool, now, "first")
        .await
        .unwrap();
    assert_eq!(first_attempt.len(), 1);
    let first_attempt = &first_attempt[0];
    assert_eq!(first_attempt.establishment_id, "one");
    assert_eq!(first_attempt.locale, "es");
    assert!(
        commerce_notifications::claim(&pool, now, "busy")
            .await
            .unwrap()
            .is_empty()
    );

    let later = now + chrono::Duration::seconds(31);
    let recovered = commerce_notifications::claim(&pool, later, "recovered")
        .await
        .unwrap();
    let recovered = &recovered[0];
    assert_eq!(recovered.event_id, first_attempt.event_id);
    assert_eq!(recovered.attempts, 2);
    commerce_notifications::record_results(
        &pool,
        &[(first_attempt, DeliveryResult::Gone)],
        "first",
        later.timestamp(),
    )
    .await
    .unwrap();
    commerce_notifications::record_results(
        &pool,
        &[(
            recovered,
            DeliveryResult::Retry {
                after_seconds: Some(10),
            },
        )],
        "recovered",
        later.timestamp(),
    )
    .await
    .unwrap();
    assert!(
        commerce_notifications::claim(&pool, later, "backoff")
            .await
            .unwrap()
            .is_empty()
    );
    let retry_time = later + chrono::Duration::seconds(10);
    let retry = commerce_notifications::claim(&pool, retry_time, "retry")
        .await
        .unwrap();
    commerce_notifications::record_results(
        &pool,
        &[(&retry[0], DeliveryResult::Delivered)],
        "retry",
        retry_time.timestamp(),
    )
    .await
    .unwrap();
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM commerce_push_deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(outcome, "delivered");
    assert!(revoke(&pool, "one", "staff", &first).await.unwrap());
}

#[tokio::test]
async fn interrupted_final_attempt_closes_after_lease_expiry() {
    let (_dir, pool) = setup().await;
    let now = Utc::now();
    register(&pool, "one", "staff", &subscription(1), now)
        .await
        .unwrap();
    publish_order(&pool, now).await;
    sqlx::query("UPDATE commerce_push_deliveries SET attempts = 7")
        .execute(&pool)
        .await
        .unwrap();
    let now = Utc::now();
    let final_attempt = commerce_notifications::claim(&pool, now, "interrupted")
        .await
        .unwrap();
    assert_eq!(final_attempt[0].attempts, 8);
    commerce_notifications::finish_expired(&pool, now.timestamp() + 29)
        .await
        .unwrap();
    let open: bool = sqlx::query_scalar("SELECT finished_at IS NULL FROM commerce_push_deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(open);
    commerce_notifications::finish_expired(&pool, now.timestamp() + 30)
        .await
        .unwrap();
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM commerce_push_deliveries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(outcome, "exhausted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "pinned release benchmark; run scripts/bench_commerce.sh push"]
async fn push_throughput() {
    let (_dir, pool) = setup().await;
    let cart = benchmark::checkout_fixture(&pool).await;
    let baseline = benchmark::checkout_p95(&pool, &cart).await;
    let now = Utc::now();
    for index in 0..1000 {
        register(&pool, "one", "staff", &subscription(index), now)
            .await
            .unwrap();
    }
    publish_order(&pool, now).await;
    let (elapsed, mixed) = tokio::join!(
        send_to_fake_provider(&pool),
        benchmark::checkout_p95(&pool, &cart)
    );
    println!("checkout_baseline_p95_ms={baseline:.3} checkout_with_push_p95_ms={mixed:.3}");
    assert!(elapsed <= 60.0 && mixed <= 300.0);
}

async fn send_to_fake_provider(pool: &SqlitePool) -> f64 {
    use crate::infrastructure::web_push::WebPushSender;
    use aws_lc_rs::rand::SystemRandom;
    use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};

    let key = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
        .unwrap();
    let sender =
        std::sync::Arc::new(WebPushSender::new(key.as_ref(), "mailto:push@example.test").unwrap());
    let active = std::sync::Arc::new(AtomicUsize::new(0));
    let peak = std::sync::Arc::new(AtomicUsize::new(0));
    let mut sent = 0;
    let start = Instant::now();
    while sent < 1000 {
        let count = process_batch(pool, |job| {
            let active = active.clone();
            let peak = peak.clone();
            let sender = sender.clone();
            async move {
                let in_flight = active.fetch_add(1, Ordering::Relaxed) + 1;
                peak.fetch_max(in_flight, Ordering::Relaxed);
                let subscriber =
                    PushSubscription::parse(&job.endpoint, &job.p256dh, &job.auth_secret).unwrap();
                let request = sender
                    .request(
                        &subscriber,
                        &job.event_id,
                        br#"{"title":"New order"}"#.to_vec(),
                        60,
                        Utc::now(),
                    )
                    .unwrap();
                assert_eq!(
                    request.headers()[reqwest::header::CONTENT_ENCODING],
                    "aes128gcm"
                );
                // A fake provider with 100 ms response time exercises bounded concurrency.
                tokio::time::sleep(Duration::from_millis(100)).await;
                active.fetch_sub(1, Ordering::Relaxed);
                (job, DeliveryResult::Delivered)
            }
        })
        .await
        .unwrap();
        assert_ne!(count, 0);
        sent += count;
    }
    let elapsed = start.elapsed().as_secs_f64();
    let maximum = peak.load(Ordering::Relaxed);
    let delivered: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commerce_push_deliveries WHERE outcome = 'delivered'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    println!(
        "deliveries={delivered} seconds={elapsed:.3} sends_per_minute={:.2} concurrency={maximum}",
        delivered as f64 * 60.0 / elapsed
    );
    assert_eq!(delivered, 1000);
    assert!(maximum <= commerce_notifications::MAX_IN_FLIGHT);
    elapsed
}
