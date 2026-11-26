use axum::Router;
use axum::http::header;
use qrcardapio::application::auth::AuthSuccess;

use super::*;

const REPLAY_PATH: &str = "/admin/outbox/replay";
const RECOVERY_PATH: &str = "/admin/outbox/recovery";
const NOTIFICATIONS: Consumer = Consumer {
    name: "commerce.notifications",
    ..MAIN
};
pub(super) const FAILING: Consumer = Consumer {
    handle: |tx, event| Box::pin(fail_selected(tx, event)),
    ..NOTIFICATIONS
};
const OTHER: Consumer = Consumer {
    name: "test.other-subscriber",
    handle: |_, _| Box::pin(async { Ok(()) }),
    ..MAIN
};

pub(super) async fn admin_login(pool: &SqlitePool) -> AuthSuccess {
    let email = "outbox-admin@example.test";
    support::admin(pool, email).await;
    auth::login(pool, &PasswordService::new(), email, "AdminPassword123")
        .await
        .unwrap()
}

pub(super) async fn mark_dead(pool: &SqlitePool) {
    let mut tx = begin_immediate(pool).await.unwrap();
    event_consumers::record_failure(&mut tx, NOTIFICATIONS.name, 1, 10, None, "error.internal")
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn dead_letter(app: &Router, admin: &AuthSuccess) -> (String, i64) {
    let response = support::get(app, admin, "/admin/outbox").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let diagnostics: Value = serde_json::from_slice(&body).unwrap();
    let letter = &diagnostics["dead_letters"][0];
    (
        letter["event_id"].as_str().unwrap().to_owned(),
        letter["replay_generation"].as_i64().unwrap(),
    )
}

fn replay_form(csrf: &str, consumer: &str, event_id: &str, generation: i64) -> String {
    format!("_csrf={csrf}&consumer={consumer}&event_id={event_id}&generation={generation}")
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct ReplayAudit {
    id: i64,
    consumer: String,
    event_seq: i64,
    actor_account_id: String,
    requested_at: i64,
    previous_error_key: String,
}

async fn audits(pool: &SqlitePool) -> Vec<ReplayAudit> {
    sqlx::query_as(
        "SELECT id, consumer, event_seq, actor_account_id, requested_at, previous_error_key
         FROM event_consumer_replays ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn admin_replay_requires_csrf_and_preserves_effects_until_dispatch() {
    let harness = init().await;
    let pool = &harness.pool;
    let app = &harness.app;
    append_events(
        pool,
        &[
            ("one", "shared", EventType::OrderCreated, true),
            ("two", "shared", EventType::OrderCreated, false),
            ("one", "shared", EventType::OrderCreated, false),
        ],
    )
    .await;
    dispatch_batch(pool, &[OTHER, FAILING], Utc::now())
        .await
        .unwrap();
    mark_dead(pool).await;
    assert_eq!(effects(pool).await, vec![2]);
    assert_eq!(unpublished(pool).await, vec![1, 3]);

    let admin = admin_login(pool).await;
    let (owner, _) = support::owner(pool, "outbox-owner").await;
    let (event_id, generation) = dead_letter(app, &admin).await;
    assert_eq!(generation, 0);
    let owner_form = replay_form(&owner.csrf_token, NOTIFICATIONS.name, &event_id, generation);
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::post(REPLAY_PATH)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(owner_form.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(support::location(&unauthenticated), "/login");
    assert_eq!(
        support::post_form(app, &owner, REPLAY_PATH, owner_form)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        support::post_form(
            app,
            &admin,
            REPLAY_PATH,
            replay_form("invalid", NOTIFICATIONS.name, &event_id, generation),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        support::post_form(
            app,
            &admin,
            REPLAY_PATH,
            replay_form(
                &admin.csrf_token,
                "commerce.payments",
                &event_id,
                generation
            ),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert!(audits(pool).await.is_empty());
    let recovery = support::get(app, &admin, RECOVERY_PATH).await;
    assert_eq!(recovery.status(), StatusCode::OK);
    assert_eq!(recovery.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(recovery.headers()["x-robots-tag"], "noindex, nofollow");
    let html = recovery.into_body().collect().await.unwrap().to_bytes();
    let html = std::str::from_utf8(&html).unwrap();
    assert!(html.contains(REPLAY_PATH) && html.contains(&event_id));
    assert!(html.contains(&admin.csrf_token));
    assert!(!html.contains("handler failure details stay private"));

    let form = replay_form(&admin.csrf_token, NOTIFICATIONS.name, &event_id, generation);
    let first_request_at = Utc::now().timestamp_millis();
    let (first, concurrent) = tokio::join!(
        support::post_form(app, &admin, REPLAY_PATH, form.clone()),
        support::post_form(app, &admin, REPLAY_PATH, form.clone()),
    );
    let (accepted, conflict) = if first.status() == StatusCode::SEE_OTHER {
        (first, concurrent)
    } else {
        (concurrent, first)
    };
    assert_eq!(accepted.status(), StatusCode::SEE_OTHER);
    assert_eq!(support::location(&accepted), RECOVERY_PATH);
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let audit = audits(pool).await;
    assert_eq!(audit.len(), 1);
    let entry = &audit[0];
    assert_eq!(entry.consumer, NOTIFICATIONS.name);
    assert_eq!(entry.event_seq, 1);
    assert_eq!(entry.actor_account_id, admin.account.id);
    assert!((first_request_at..=Utc::now().timestamp_millis()).contains(&entry.requested_at));
    assert_eq!(entry.previous_error_key, "error.internal");
    assert_eq!(effects(pool).await, vec![2]);
    assert_eq!(unpublished(pool).await, vec![1, 3]);
    let failure: (String, i64) = sqlx::query_as(
        "SELECT status, attempts FROM event_consumer_failures WHERE consumer = ? AND event_seq = 1",
    )
    .bind(NOTIFICATIONS.name)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(failure, ("parked".to_owned(), 0));

    let now = Utc::now();
    dispatch_batch(pool, &[OTHER, NOTIFICATIONS], now)
        .await
        .unwrap();
    assert_eq!(effects(pool).await, vec![2, 1]);
    assert_eq!(unpublished(pool).await, vec![3]);
    dispatch_batch(pool, &[OTHER, NOTIFICATIONS], now)
        .await
        .unwrap();
    assert_eq!(effects(pool).await, vec![2, 1, 3]);
    assert!(unpublished(pool).await.is_empty());
    let receipts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT consumer, COUNT(*) FROM event_consumer_receipts GROUP BY consumer ORDER BY consumer",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        receipts,
        vec![
            (NOTIFICATIONS.name.to_owned(), 3),
            (OTHER.name.to_owned(), 3)
        ]
    );
    assert_eq!(
        support::post_form(app, &admin, REPLAY_PATH, form)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(audits(pool).await, audit);
}

#[tokio::test]
async fn stale_replay_form_cannot_restart_a_later_dead_letter_cycle() {
    let harness = init().await;
    let pool = &harness.pool;
    let app = &harness.app;
    append_events(pool, &[("one", "order", EventType::OrderCreated, true)]).await;
    dispatch_batch(pool, &[FAILING], Utc::now()).await.unwrap();
    mark_dead(pool).await;
    let admin = admin_login(pool).await;
    let (event_id, initial_generation) = dead_letter(app, &admin).await;
    let initial_form = replay_form(
        &admin.csrf_token,
        NOTIFICATIONS.name,
        &event_id,
        initial_generation,
    );
    assert_eq!(
        support::post_form(app, &admin, REPLAY_PATH, initial_form.clone())
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    dispatch_batch(pool, &[FAILING], Utc::now()).await.unwrap();
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM event_consumer_failures WHERE consumer = ? AND event_seq = 1 AND status = 'retry'",
    )
    .bind(NOTIFICATIONS.name)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(attempts, 1);
    assert!(effects(pool).await.is_empty());
    mark_dead(pool).await;
    let (same_id, latest_generation) = dead_letter(app, &admin).await;
    assert_eq!(same_id, event_id);
    let original_audit = audits(pool).await;
    assert_eq!(latest_generation, original_audit[0].id);
    assert!(latest_generation > initial_generation);

    assert_eq!(
        support::post_form(app, &admin, REPLAY_PATH, initial_form)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(dead_letter(app, &admin).await.1, latest_generation);
    assert_eq!(audits(pool).await, original_audit);
    let fresh_form = replay_form(
        &admin.csrf_token,
        NOTIFICATIONS.name,
        &event_id,
        latest_generation,
    );
    assert_eq!(
        support::post_form(app, &admin, REPLAY_PATH, fresh_form)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let audit = audits(pool).await;
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0], original_audit[0]);
    assert!(audit[1].id > latest_generation);
}
