use qrcardapio::application::publishing::{
    PublishLocks, publish_establishment, republish_all, write_static_snapshot,
};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    capture_menu_publication, find_establishment_by_id, record_publication_success,
    update_product_for_est,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

const EDIT_TIME: &str = "2026-11-25T15:00:00Z";

async fn edit_product(pool: &SqlitePool, name: &str) {
    assert!(
        update_product_for_est(
            pool, "a", "dish", "cat", name, "", "", "", "", "", 1000, None, true, true, EDIT_TIME,
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn edit_during_publish_stays_private_on_republish_all_even_with_equal_timestamps() {
    let dir = TempDir::new().unwrap();
    let cfg = Config {
        payments: Default::default(),
        asaas: None,
        mcp_enabled: false,
        google: None,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "https://qrcardapio.com.br".into(),
        data_dir: dir.path().join("data"),
        static_dir: dir.path().join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".into(),
    };
    let pool = init_pool(&cfg).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO establishments (id, slug, name, created_at, updated_at)
         VALUES ('a', 'menu-draft-a', 'Estabelecimento A', 'initial', 'initial');
         INSERT INTO categories (id, establishment_id, name, position, created_at, updated_at)
         VALUES ('cat', 'a', 'Pratos', 0, 'initial', 'initial');
         INSERT INTO products (id, establishment_id, category_id, name, price_centavos, position, created_at, updated_at)
         VALUES ('dish', 'a', 'cat', 'Prato inicial', 1000, 0, 'initial', 'initial');",
    )
    .execute(&pool)
    .await
    .unwrap();

    let locks = PublishLocks::default();
    publish_establishment(&pool, &cfg, &locks, "a")
        .await
        .unwrap();
    edit_product(&pool, "Prato aprovado").await;
    let captured = capture_menu_publication(&pool, "a").await.unwrap().unwrap();
    assert_eq!(captured.snapshot.establishment.updated_at, EDIT_TIME);
    assert!(captured.snapshot.establishment.has_unpublished_changes);

    // Commit an independent edit between capturing the menu and finishing publication.
    edit_product(&pool, "Prato em segredo").await;
    let draft = find_establishment_by_id(&pool, "a").await.unwrap().unwrap();
    assert_eq!(draft.updated_at, EDIT_TIME);
    let (bytes, assets) = write_static_snapshot(&cfg, &captured.snapshot)
        .await
        .unwrap();
    record_publication_success(
        &pool,
        &captured,
        2,
        bytes as i64,
        assets as i64,
        0,
        EDIT_TIME,
    )
    .await
    .unwrap();

    let live_path = cfg.published_dir().join("menu-draft-a/index.html");
    let live = std::fs::read_to_string(&live_path).unwrap();
    assert!(live.contains("Prato aprovado"));
    assert!(!live.contains("Prato em segredo"));
    assert!(
        find_establishment_by_id(&pool, "a")
            .await
            .unwrap()
            .unwrap()
            .has_unpublished_changes
    );
    assert_eq!(republish_all(&pool, &cfg, &locks).await.unwrap(), 0);
    assert_eq!(std::fs::read_to_string(&live_path).unwrap(), live);

    publish_establishment(&pool, &cfg, &locks, "a")
        .await
        .unwrap();
    assert!(
        std::fs::read_to_string(&live_path)
            .unwrap()
            .contains("Prato em segredo")
    );
    assert!(
        !find_establishment_by_id(&pool, "a")
            .await
            .unwrap()
            .unwrap()
            .has_unpublished_changes
    );
}
