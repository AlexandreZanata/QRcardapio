use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;
use tokio::time::{MissedTickBehavior, interval, sleep};

use super::{notifications, order_expiry, payments};
use crate::domain::events::EventType;
use crate::infrastructure::database::queries::{checkout_attempts, commerce_notifications};
use crate::infrastructure::events::dispatcher::{Consumer, dispatch_batch, wait_for_commit};
use crate::infrastructure::web_push::WebPushSender;

pub const CONSUMERS: &[Consumer] = &[
    Consumer {
        name: "commerce.payments",
        event_types: &[EventType::OrderRejected, EventType::OrderCancelled],
        handle: |tx, event| Box::pin(payments::compensate_cancelled_order_in(tx, event)),
    },
    Consumer {
        name: "commerce.notifications",
        event_types: &[EventType::OrderCreated],
        handle: |tx, event| Box::pin(notifications::enqueue_order(tx, event)),
    },
];

pub async fn run(pool: &SqlitePool, sender: Option<WebPushSender>) {
    if let Some(sender) = sender {
        tokio::join!(process_events(pool), deliver_push(pool, Arc::new(sender)));
    } else {
        process_events(pool).await;
    }
}

async fn process_events(pool: &SqlitePool) {
    let mut dispatch = interval(Duration::from_millis(250));
    let mut deadlines = interval(Duration::from_secs(1));
    let mut cleanup = interval(Duration::from_secs(15));
    for timer in [&mut dispatch, &mut deadlines, &mut cleanup] {
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    }
    loop {
        tokio::select! {
            _ = async {
                tokio::select! {
                    _ = dispatch.tick() => (),
                    // Group events from contiguous order writes before competing for the writer.
                    _ = wait_for_commit() => sleep(Duration::from_millis(100)).await,
                }
            } => {
                for _ in 0..8 {
                    match dispatch_batch(pool, CONSUMERS, Utc::now()).await {
                        Ok(0) => break,
                        Ok(_) => (),
                        Err(error) => {
                            tracing::warn!(%error, "Commerce outbox dispatch failed");
                            break;
                        }
                    }
                }
            }
            _ = deadlines.tick() => {
                for _ in 0..8 {
                    match order_expiry::expire_due(pool, Utc::now()).await {
                        Ok(0) => break,
                        Ok(_) => (),
                        Err(error) => {
                            tracing::warn!(%error, "Commerce acceptance expiry failed");
                            break;
                        }
                    }
                }
            }
            _ = cleanup.tick() => {
                let now = Utc::now();
                for _ in 0..8 {
                    match checkout_attempts::prune_expired(pool, now, 1000).await {
                        Ok(count) if count < 1000 => break,
                        Ok(_) => (),
                        Err(error) => {
                            tracing::warn!(%error, "Commerce checkout cleanup failed");
                            break;
                        }
                    }
                }
                let cutoff = (now - chrono::Duration::days(7)).timestamp();
                if let Err(error) = commerce_notifications::finish_expired(pool, now.timestamp()).await {
                    tracing::warn!(%error, "Commerce notification expiry failed");
                }
                if let Err(error) = commerce_notifications::prune(pool, cutoff).await {
                    tracing::warn!(%error, "Commerce notification cleanup failed");
                }
            }
        }
    }
}

async fn deliver_push(pool: &SqlitePool, sender: Arc<WebPushSender>) {
    loop {
        match notifications::send_pending(pool, &sender).await {
            Ok(count) if count == commerce_notifications::MAX_IN_FLIGHT => continue,
            Ok(_) => (),
            Err(error) => tracing::warn!(%error, "Commerce notification batch failed"),
        }
        sleep(Duration::from_millis(250)).await;
    }
}
