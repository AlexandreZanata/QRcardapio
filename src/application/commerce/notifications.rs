#[cfg(test)]
mod tests;

use std::future::Future;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale};
use crate::infrastructure::database::queries::{
    commerce_notifications, domain_events::StoredEvent,
};
use crate::infrastructure::web_push::{DeliveryResult, PushSubscription, WebPushSender};
use commerce_notifications::PushDelivery;

/// Called by an authorized staff adapter with the current session's membership.
pub async fn register(
    pool: &SqlitePool,
    establishment_id: &str,
    account_id: &str,
    subscription: &PushSubscription,
    now: DateTime<Utc>,
) -> AppResult<String> {
    commerce_notifications::register(pool, establishment_id, account_id, subscription, now)
        .await?
        .ok_or(AppError::Forbidden("error.forbidden"))
}

pub async fn revoke(
    pool: &SqlitePool,
    establishment_id: &str,
    account_id: &str,
    subscription_id: &str,
) -> AppResult<bool> {
    commerce_notifications::revoke(pool, establishment_id, account_id, subscription_id).await
}

pub(crate) async fn enqueue_order(
    tx: &mut Transaction<'_, Sqlite>,
    event: &StoredEvent,
) -> AppResult<()> {
    if event.event_version != 1 || event.aggregate_type != "order" {
        return Err(AppError::Internal(
            "invalid notification event envelope".into(),
        ));
    }
    commerce_notifications::enqueue(tx, event, Utc::now()).await
}

pub(crate) async fn send_pending(
    pool: &SqlitePool,
    sender: &Arc<WebPushSender>,
) -> AppResult<usize> {
    process_batch(pool, |job| {
        let sender = sender.clone();
        async move {
            let result = match PushSubscription::parse(&job.endpoint, &job.p256dh, &job.auth_secret)
            {
                Ok(subscription) => {
                    let locale = Locale::parse(&job.locale).unwrap_or_default();
                    let strings = Catalog::global();
                    let body = strings
                        .t(locale, "commerce.push.order_body")
                        .replace("{number}", &job.order_number.to_string());
                    let content = json!({
                        "title": strings.t(locale, "commerce.push.order_title"),
                        "body": body,
                        "tag": job.event_id,
                    });
                    sender
                        .send(
                            &subscription,
                            &job.event_id,
                            content.to_string().into_bytes(),
                            job.remaining_seconds,
                        )
                        .await
                }
                Err(_) => DeliveryResult::Rejected,
            };
            (job, result)
        }
    })
    .await
}

async fn process_batch<F>(pool: &SqlitePool, send: impl Fn(PushDelivery) -> F) -> AppResult<usize>
where
    F: Future<Output = (PushDelivery, DeliveryResult)> + Send + 'static,
{
    let token = Uuid::now_v7().to_string();
    let jobs = commerce_notifications::claim(pool, Utc::now(), &token).await?;
    let count = jobs.len();
    let mut sends = JoinSet::new();
    for job in jobs {
        sends.spawn(send(job));
    }
    let mut results = Vec::with_capacity(count);
    let mut send_failed = false;
    while let Some(done) = sends.join_next().await {
        match done {
            Ok(result) => results.push(result),
            Err(_) => send_failed = true,
        }
    }
    if !results.is_empty() {
        let receipts: Vec<_> = results.iter().map(|(job, result)| (job, *result)).collect();
        commerce_notifications::record_results(pool, &receipts, &token, Utc::now().timestamp())
            .await?;
    }
    if send_failed {
        return Err(AppError::Internal("Web Push send task failed".into()));
    }
    Ok(count)
}
