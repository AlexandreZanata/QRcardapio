//! Transactional delivery of the SQLite outbox to registered in-process consumers.

use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{Acquire, Sqlite, SqlitePool, Transaction};

use crate::domain::events::EventType;
use crate::error::AppResult;
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::domain_events::StoredEvent;
use crate::infrastructure::database::queries::{event_consumers, event_publications};

const BATCH_SIZE: i64 = 200;
const MAX_ATTEMPTS: i64 = 10;
const BACKOFF_MS: [i64; 6] = [1_000, 5_000, 30_000, 120_000, 600_000, 3_600_000];
static COMMITTED: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Wake the in-process worker after the producer commits the event transaction.
pub(crate) fn notify_committed() {
    COMMITTED.notify_one();
}

pub(crate) async fn wait_for_commit() {
    COMMITTED.notified().await;
}

pub type ConsumerFuture<'a> = Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>>;
type Handler =
    for<'a, 'tx> fn(&'a mut Transaction<'tx, Sqlite>, &'a StoredEvent) -> ConsumerFuture<'a>;

#[derive(Clone, Copy)]
pub struct Consumer {
    pub name: &'static str,
    pub event_types: &'static [EventType],
    /// Persist database effects through this transaction. External calls belong to a
    /// separate sender reading an outbound record committed by this handler.
    pub handle: Handler,
}

/// Scans up to 200 events per consumer and finalizes one batch of acknowledged publications.
/// The checkpoint tracks the scan. Receipts and the retry queue track delivery, including gaps.
pub async fn dispatch_batch(
    pool: &SqlitePool,
    consumers: &[Consumer],
    now: DateTime<Utc>,
) -> AppResult<usize> {
    let now_ms = now.timestamp_millis();
    let head = event_consumers::latest_seq(pool).await?;
    let mut progress = 0;
    let mut subscriptions = Vec::with_capacity(consumers.len());
    for consumer in consumers {
        let event_types: Vec<_> = consumer
            .event_types
            .iter()
            .map(|kind| kind.as_str())
            .collect();
        let checkpoint = event_consumers::checkpoint(pool, consumer.name).await?;
        if checkpoint.is_none_or(|last| last < head)
            || event_consumers::has_due_retry(pool, consumer.name, now_ms).await?
        {
            progress +=
                scan_consumer(pool, consumer, now_ms, &json!(event_types).to_string()).await?;
        }
        subscriptions.push(json!({ "consumer": consumer.name, "event_types": event_types }));
    }
    progress +=
        event_publications::mark_ready(pool, &json!(subscriptions).to_string(), now_ms, BATCH_SIZE)
            .await?;
    Ok(progress)
}

async fn scan_consumer(
    pool: &SqlitePool,
    consumer: &Consumer,
    now_ms: i64,
    types_json: &str,
) -> AppResult<usize> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let last_seq = match event_consumers::checkpoint(&mut *tx, consumer.name).await? {
        Some(seq) => seq,
        None => {
            event_consumers::advance(&mut tx, consumer.name, 0).await?;
            0
        }
    };
    let head = event_consumers::latest_seq(&mut *tx).await?;
    let retries = event_consumers::due_retries(&mut tx, consumer.name, now_ms, BATCH_SIZE).await?;
    let mut progress = retries.len();
    let budget = BATCH_SIZE - progress as i64;
    for retry in retries {
        deliver(&mut tx, consumer, &retry.event, retry.attempts + 1, now_ms).await?;
    }
    if budget > 0 && head > last_seq {
        let new_events =
            event_consumers::unscanned(&mut tx, types_json, last_seq, head, budget).await?;
        let scanned_through = if new_events.len() < budget as usize {
            head
        } else {
            new_events.last().map_or(last_seq, |event| event.seq)
        };
        progress += new_events.len();
        for event in new_events {
            if event_consumers::is_blocked(&mut tx, consumer.name, event.seq).await? {
                event_consumers::park(&mut tx, consumer.name, event.seq).await?;
            } else {
                deliver(&mut tx, consumer, &event, 1, now_ms).await?;
            }
        }
        event_consumers::advance(&mut tx, consumer.name, scanned_through).await?;
    }
    tx.commit().await?;
    Ok(progress)
}

async fn deliver(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &Consumer,
    event: &StoredEvent,
    attempt: i64,
    now_ms: i64,
) -> AppResult<()> {
    // A failed handler discards its partial writes without discarding other aggregates'
    // work in the bounded transaction. A crash before outer commit drops the entire batch.
    let mut savepoint = tx.begin().await?;
    if event_consumers::claim(&mut savepoint, consumer.name, &event.event_id, now_ms).await?
        && let Err(err) = (consumer.handle)(&mut savepoint, event).await
    {
        savepoint.rollback().await?;
        let retry_at = retry_at(now_ms, event.seq, attempt);
        event_consumers::record_failure(
            &mut *tx,
            consumer.name,
            event.seq,
            attempt,
            retry_at,
            err.i18n_key(),
        )
        .await?;
        if retry_at.is_none() {
            tracing::warn!(
                consumer = consumer.name,
                seq = event.seq,
                "outbox event moved to dead letter"
            );
        }
    } else {
        savepoint.commit().await?;
        event_consumers::clear_failure(tx, consumer.name, event.seq).await?;
    }
    Ok(())
}

fn retry_at(now_ms: i64, seq: i64, attempt: i64) -> Option<i64> {
    if attempt >= MAX_ATTEMPTS {
        return None;
    }
    let backoff = BACKOFF_MS[((attempt - 1) as usize).min(BACKOFF_MS.len() - 1)];
    // Stable per-event jitter avoids synchronized retries and keeps replay timing repeatable.
    let jitter = seq
        .wrapping_mul(2_147)
        .wrapping_add(attempt * 9_973)
        .rem_euclid(backoff / 4 + 1);
    Some(now_ms.saturating_add(backoff + jitter))
}
