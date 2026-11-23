use std::path::PathBuf;

use tempfile::TempDir;

use qrcardapio::application::auth::register_owner;
use qrcardapio::application::establishments::{CreateEstablishmentInput, create_establishment};
use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product,
};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::queries::{
    find_establishment_for_account, list_categories_for_est, list_products_for_est,
};
use qrcardapio::infrastructure::database::{
    backup_sqlite_into, init_pool, query_sqlite_version, restore_sqlite_from,
};
use qrcardapio::infrastructure::security::PasswordService;

fn test_config(dir: &TempDir) -> Config {
    Config {
        asaas: None,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: "http://127.0.0.1:8080".to_string(),
        data_dir: dir.path().join("data"),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 2,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_string(),
    }
}

#[tokio::test]
async fn online_vacuum_into_backup_and_verified_restore() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());

    let sqlite_ver = query_sqlite_version(&pool).await.unwrap();
    assert!(sqlite_ver.starts_with("3.54."));

    let auth = register_owner(
        &pool,
        &pw,
        "backup@lanchonete.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    let est = create_establishment(
        &pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: "Lanchonete Central",
            slug: "lanchonete-central",
            description: "Lanches rápidos",
            accent_color: "#D9532F",
        },
    )
    .await
    .unwrap();
    let cat = create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Burgers",
            name_en: "Burgers",
            name_es: "Hamburguesas",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &cat.id,
            name: "X-Bacon",
            name_en: "Bacon Burger",
            name_es: "Hamburguesa con Tocino",
            description: "Pão brioche",
            description_en: "Brioche bun",
            description_es: "Pan brioche",
            price_raw: "27,90",
            is_available: true,
            is_visible: true,
        },
        None,
    )
    .await
    .unwrap();

    // Perform online VACUUM INTO backup while database pool is open and active.
    let backup_file = dir.path().join("backups/snapshot.sqlite");
    backup_sqlite_into(&pool, &backup_file).await.unwrap();
    assert!(backup_file.exists());

    // Simulate post-backup mutation that should disappear after restoring the snapshot.
    create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Categoria Pós Backup",
            name_en: "",
            name_es: "",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        list_categories_for_est(&pool, &est.id).await.unwrap().len(),
        2
    );

    pool.close().await;

    // Restore from backup and reopen pool.
    restore_sqlite_from(&backup_file, &cfg.db_path())
        .await
        .unwrap();
    let restored_pool = init_pool(&cfg).await.unwrap();

    let restored_est = find_establishment_for_account(&restored_pool, &auth.account.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored_est.slug, "lanchonete-central");

    let cats = list_categories_for_est(&restored_pool, &restored_est.id)
        .await
        .unwrap();
    assert_eq!(cats.len(), 1);
    assert_eq!(cats[0].name, "Burgers");

    let prods = list_products_for_est(&restored_pool, &restored_est.id)
        .await
        .unwrap();
    assert_eq!(prods.len(), 1);
    assert_eq!(prods[0].price_centavos, 2790);

    restored_pool.close().await;
}
