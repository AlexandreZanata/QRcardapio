//! Transactional delivery of the SQLite outbox to registered in-process consumers.

use std::collections::HashSet;
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

/// Scans up to 200 events per consumer and commits their effects and publication together.
/// The Commerce write queue yields to other writers between batches.
pub async fn dispatch_batch(
    pool: &SqlitePool,
    consumers: &[Consumer],
    now: DateTime<Utc>,
) -> AppResult<usize> {
    let now_ms = now.timestamp_millis();
    let event_types: Vec<Vec<&str>> = consumers
        .iter()
        .map(|consumer| {
            consumer
                .event_types
                .iter()
                .map(|kind| kind.as_str())
                .collect()
        })
        .collect();
    let subscriptions = consumers
        .iter()
        .zip(&event_types)
        .map(|(consumer, types)| json!({ "consumer": consumer.name, "event_types": types }))
        .collect();
    let subscriptions = serde_json::Value::Array(subscriptions).to_string();
    if !event_consumers::has_work(pool, &subscriptions, now_ms).await?
        && !event_publications::has_ready(pool, &subscriptions).await?
    {
        return Ok(0);
    }

    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let head = event_consumers::latest_seq(&mut *tx).await?;
    let mut progress = 0;
    for (consumer, types) in consumers.iter().zip(&event_types) {
        progress +=
            scan_consumer(&mut tx, consumer, head, now_ms, &json!(types).to_string()).await?;
    }
    progress += event_publications::mark_ready(&mut tx, &subscriptions, now_ms, BATCH_SIZE).await?;
    tx.commit().await?;
    Ok(progress)
}

async fn scan_consumer(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &Consumer,
    head: i64,
    now_ms: i64,
    types_json: &str,
) -> AppResult<usize> {
    let last_seq = match event_consumers::checkpoint(&mut **tx, consumer.name).await? {
        Some(seq) => seq,
        None => {
            event_consumers::advance(tx, consumer.name, 0).await?;
            0
        }
    };
    let mut progress = scan_retries(tx, consumer, now_ms).await?;
    let budget = BATCH_SIZE - progress as i64;
    if budget > 0 && head > last_seq {
        let new_events =
            event_consumers::unscanned(tx, consumer.name, types_json, last_seq, head, budget)
                .await?;
        let scanned_through = if new_events.len() < budget as usize {
            head
        } else {
            new_events
                .last()
                .map_or(last_seq, |scanned| scanned.event.seq)
        };
        progress += new_events.len();
        scan_fresh(tx, consumer, &new_events, now_ms).await?;
        event_consumers::advance(tx, consumer.name, scanned_through).await?;
    }
    Ok(progress)
}

async fn scan_retries(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &Consumer,
    now_ms: i64,
) -> AppResult<usize> {
    let retries = event_consumers::due_retries(tx, consumer.name, now_ms, BATCH_SIZE).await?;
    let mut received = Vec::with_capacity(retries.len());
    for retry in &retries {
        if !retry.received {
            if !deliver(tx, consumer, &retry.event, retry.attempts + 1, now_ms).await? {
                continue;
            }
            received.push(retry.event.event_id.as_str());
        }
        event_consumers::clear_failure(tx, consumer.name, retry.event.seq).await?;
    }
    event_consumers::record_receipts(tx, consumer.name, &received, now_ms).await?;
    Ok(retries.len())
}

async fn scan_fresh(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &Consumer,
    rows: &[event_consumers::ScannedEvent],
    now_ms: i64,
) -> AppResult<()> {
    let mut received = Vec::with_capacity(rows.len());
    let mut new_failures = HashSet::new();
    for row in rows {
        let event = &row.event;
        let aggregate = (
            &event.establishment_id,
            &event.aggregate_type,
            &event.aggregate_id,
        );
        if row.blocked || new_failures.contains(&aggregate) {
            event_consumers::park(tx, consumer.name, event.seq).await?;
        } else if !row.received {
            if deliver(tx, consumer, event, 1, now_ms).await? {
                received.push(event.event_id.as_str());
            } else {
                new_failures.insert(aggregate);
            }
        }
    }
    event_consumers::record_receipts(tx, consumer.name, &received, now_ms).await
}

async fn deliver(
    tx: &mut Transaction<'_, Sqlite>,
    consumer: &Consumer,
    event: &StoredEvent,
    attempt: i64,
    now_ms: i64,
) -> AppResult<bool> {
    // A failed handler discards its partial writes without discarding other aggregates'
    // work in the bounded transaction. A crash before outer commit drops the entire batch.
    let mut savepoint = tx.begin().await?;
    let Err(err) = (consumer.handle)(&mut savepoint, event).await else {
        savepoint.commit().await?;
        return Ok(true);
    };
    savepoint.rollback().await?;
    let retry_at = retry_at(now_ms, event.seq, attempt);
    event_consumers::record_failure(
        tx,
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
    Ok(false)
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
