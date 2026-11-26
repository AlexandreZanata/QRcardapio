use qrcardapio::config::DbSynchronous;
use qrcardapio::domain::commercial::catalog;
use qrcardapio::infrastructure::database::queries::commercial_catalog;
use qrcardapio::infrastructure::database::{MIGRATOR, open_pool_at};
use sqlx::migrate::Migrator;
use tempfile::tempdir;

const LEGACY_RELEASE: i64 = 20261124000010;
const NOTIFICATIONS_RELEASE: i64 = 20261125000018;
const REPLAY_RELEASE: i64 = 20261125000019;

#[tokio::test]
async fn expand_migrations_preserve_rows_and_allow_previous_startup() {
    let root = tempdir().unwrap();
    let path = root.path().join("rollback.sqlite");
    let pool = open_pool_at(&path, 2, DbSynchronous::Full).await.unwrap();
    MIGRATOR.run_to(LEGACY_RELEASE, &pool).await.unwrap();
    commercial_catalog::sync_registry(&pool, catalog())
        .await
        .unwrap();

    sqlx::raw_sql(
        "INSERT INTO establishments (id, slug, name, is_published, published_version, created_at, updated_at)
         VALUES ('old-est', 'old-est', 'Existing menu', 1, 3, '2026-11-24', '2026-11-24');
         INSERT INTO categories (id, establishment_id, name, position, created_at, updated_at)
         VALUES ('old-cat', 'old-est', 'Existing category', 0, '2026-11-24', '2026-11-24');
         INSERT INTO products (id, establishment_id, category_id, name, price_centavos, position, created_at, updated_at)
         VALUES ('old-item', 'old-est', 'old-cat', 'Existing item', 1250, 0, '2026-11-24', '2026-11-24');",
    )
    .execute(&pool)
    .await
    .unwrap();

    MIGRATOR.run_to(NOTIFICATIONS_RELEASE, &pool).await.unwrap();
    let event_seq: i64 = sqlx::query_scalar(
        "INSERT INTO domain_events (
             event_id, event_type, event_version, establishment_id, aggregate_type,
             aggregate_id, correlation_id, occurred_at, payload
         ) VALUES (
             'old-event', 'OrderRejected', 1, 'old-est', 'order',
             'old-order', 'old-event', '2026-11-25T12:00:00Z', '{\"order_id\":\"old-order\"}'
         ) RETURNING seq",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO event_consumer_checkpoints VALUES ('commerce.payments', ?)")
        .bind(event_seq)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO event_consumer_failures (consumer, event_seq, status, attempts, error_key)
         VALUES ('commerce.payments', ?, 'dead', 10, 'commerce.payment_state_conflict')",
    )
    .bind(event_seq)
    .execute(&pool)
    .await
    .unwrap();

    MIGRATOR.run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO event_consumer_replays (
             id, consumer, event_seq, actor_account_id, requested_at, previous_error_key
         ) VALUES (1, 'commerce.payments', ?, 'old-admin', 1795608000000, 'commerce.payment_state_conflict')",
    )
    .bind(event_seq)
    .execute(&pool)
    .await
    .unwrap();

    for migration in MIGRATOR.iter().filter(|row| row.version > LEGACY_RELEASE) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;

    // Exercise init_pool's migration and registry initialization with the embedded
    // migration lists of the legacy and immediately preceding binaries.
    let pool = open_pool_at(&path, 2, DbSynchronous::Full).await.unwrap();
    for version in [LEGACY_RELEASE, NOTIFICATIONS_RELEASE, REPLAY_RELEASE] {
        let mut previous = Migrator::with_migrations(
            MIGRATOR
                .iter()
                .filter(|row| row.version <= version)
                .cloned()
                .collect(),
        );
        previous.set_ignore_missing(true);
        previous.run(&pool).await.unwrap();
        commercial_catalog::sync_registry(&pool, catalog())
            .await
            .unwrap();
    }
    let row: (String, i64, i64) = sqlx::query_as(
        "SELECT p.name, p.price_centavos, e.published_version
         FROM products p JOIN establishments e ON e.id = p.establishment_id
         WHERE p.id = ? AND p.establishment_id = ?",
    )
    .bind("old-item")
    .bind("old-est")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("Existing item".into(), 1250, 3));
    let failure: (String, i64, String) = sqlx::query_as(
        "SELECT status, attempts, error_key FROM event_consumer_failures
         WHERE consumer = 'commerce.payments' AND event_seq = ?",
    )
    .bind(event_seq)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        failure,
        ("dead".into(), 10, "commerce.payment_state_conflict".into())
    );
    let replays: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, actor_account_id FROM event_consumer_replays
         WHERE consumer = 'commerce.payments' AND event_seq = ?",
    )
    .bind(event_seq)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(replays, [(1, "old-admin".into())]);
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}
