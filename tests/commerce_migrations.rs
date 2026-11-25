use qrcardapio::config::DbSynchronous;
use qrcardapio::domain::commercial::catalog;
use qrcardapio::infrastructure::database::queries::commercial_catalog;
use qrcardapio::infrastructure::database::{MIGRATOR, open_pool_at};
use sqlx::migrate::Migrator;
use tempfile::tempdir;

const PREVIOUS_RELEASE: i64 = 20261124000010;

#[tokio::test]
async fn expand_migrations_preserve_rows_and_allow_previous_startup() {
    let root = tempdir().unwrap();
    let path = root.path().join("rollback.sqlite");
    let pool = open_pool_at(&path, 2, DbSynchronous::Full).await.unwrap();
    MIGRATOR.run_to(PREVIOUS_RELEASE, &pool).await.unwrap();
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

    MIGRATOR.run(&pool).await.unwrap();
    for migration in MIGRATOR.iter().filter(|row| row.version > PREVIOUS_RELEASE) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;

    // Exercise the same migration and registry initialization as init_pool with the
    // embedded migration list the previous binary knows about.
    let pool = open_pool_at(&path, 2, DbSynchronous::Full).await.unwrap();
    let mut previous = Migrator::with_migrations(
        MIGRATOR
            .iter()
            .filter(|row| row.version <= PREVIOUS_RELEASE)
            .cloned()
            .collect(),
    );
    previous.set_ignore_missing(true);
    previous.run(&pool).await.unwrap();
    commercial_catalog::sync_registry(&pool, catalog())
        .await
        .unwrap();
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
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}
