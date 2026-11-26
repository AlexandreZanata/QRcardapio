use qrcardapio::application::publishing::{PublishLocks, republish_all, write_static_snapshot};
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::queries::{
    addon_groups, addon_options, catalog_snapshots, find_establishment_by_id, load_menu_snapshot,
    product_variations, update_product_for_est,
};
use qrcardapio::infrastructure::database::{MIGRATOR, init_pool, open_pool_at};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[path = "commerce_catalog/fixtures.rs"]
mod fixtures;
use fixtures::{NOW, config, group, option, seed, variation};

const CHANNEL: &str = "storefront";

#[tokio::test]
async fn option_queries_and_foreign_keys_enforce_establishment_boundaries() {
    let dir = TempDir::new().unwrap();
    let pool = init_pool(&config(&dir)).await.unwrap();
    seed(&pool).await;

    let variation = variation();
    let option = option();
    let group_a = group("g-a");
    let group_b = group("g-b");
    addon_groups::save(&pool, "a", &group_a, NOW).await.unwrap();
    addon_groups::save(&pool, "b", &group_b, NOW).await.unwrap();
    product_variations::save(&pool, "a", "a-product", &variation, NOW)
        .await
        .unwrap();
    addon_options::save(&pool, "a", "g-a", &option, NOW)
        .await
        .unwrap();
    addon_groups::link_to_product(&pool, "a", "a-product", "g-a", 0, NOW)
        .await
        .unwrap();
    assert!(
        find_establishment_by_id(&pool, "a")
            .await
            .unwrap()
            .unwrap()
            .has_unpublished_changes
    );

    for result in [
        product_variations::save(&pool, "a", "b-product", &variation, NOW).await,
        product_variations::save(&pool, "b", "b-product", &variation, NOW).await,
        addon_groups::save(&pool, "b", &group_a, NOW).await,
        addon_options::save(&pool, "a", "g-b", &option, NOW).await,
        addon_options::save(&pool, "b", "g-b", &option, NOW).await,
        addon_groups::link_to_product(&pool, "a", "a-product", "g-b", 0, NOW).await,
        addon_groups::link_to_product(&pool, "a", "b-product", "g-a", 0, NOW).await,
    ] {
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }
    assert!(
        product_variations::list_for_product(&pool, "b", "a-product")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        addon_options::list_for_group(&pool, "b", "g-a")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        addon_groups::list_for_establishment(&pool, "b")
            .await
            .unwrap(),
        vec![group_b]
    );
    assert!(
        !product_variations::delete(&pool, "b", "a-product", "v", NOW)
            .await
            .unwrap()
    );
    assert!(
        !addon_options::delete(&pool, "b", "g-a", "o", NOW)
            .await
            .unwrap()
    );
    assert!(!addon_groups::delete(&pool, "b", "g-a", NOW).await.unwrap());
    assert!(
        !addon_groups::unlink_from_product(&pool, "b", "a-product", "g-a", NOW)
            .await
            .unwrap()
    );

    for statement in [
        "INSERT INTO product_variations (id, establishment_id, product_id, name, price_centavos, position)
         VALUES ('invalid', 'a', 'b-product', 'Foreign', 0, 0)",
        "INSERT INTO addon_options (id, establishment_id, group_id, name, price_centavos, position, max_quantity)
         VALUES ('invalid', 'a', 'g-b', 'Foreign', 0, 0, 1)",
        "INSERT INTO product_addon_groups (establishment_id, product_id, group_id, position)
         VALUES ('a', 'a-product', 'g-b', 0)",
    ] {
        let error = sqlx::query(statement).execute(&pool).await.unwrap_err();
        assert!(error.as_database_error().unwrap().is_foreign_key_violation());
    }

    assert_eq!(
        product_variations::list_for_product(&pool, "a", "a-product")
            .await
            .unwrap(),
        vec![variation]
    );
    assert_eq!(
        addon_options::list_for_group(&pool, "a", "g-a")
            .await
            .unwrap(),
        vec![option]
    );
    assert!(addon_groups::delete(&pool, "a", "g-a", NOW).await.unwrap());
    assert!(
        addon_options::list_for_group(&pool, "a", "g-a")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !addon_groups::unlink_from_product(&pool, "a", "a-product", "g-a", NOW)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn catalog_snapshots_are_canonical_scoped_versioned_and_derived_only_from_published_content()
{
    let dir = TempDir::new().unwrap();
    let pool = init_pool(&config(&dir)).await.unwrap();
    seed(&pool).await;
    let mut variation = variation();
    product_variations::save(&pool, "a", "a-product", &variation, NOW)
        .await
        .unwrap();
    addon_groups::save(&pool, "a", &group("g-a"), NOW)
        .await
        .unwrap();
    addon_groups::save(&pool, "a", &group("g-unlinked"), NOW)
        .await
        .unwrap();
    addon_options::save(&pool, "a", "g-a", &option(), NOW)
        .await
        .unwrap();
    addon_groups::link_to_product(&pool, "a", "a-product", "g-a", 0, NOW)
        .await
        .unwrap();
    assert!(
        catalog_snapshots::current(&pool, "a", CHANNEL)
            .await
            .unwrap()
            .is_none()
    );

    let (first, second) = tokio::join!(
        catalog_snapshots::publish(&pool, "a", CHANNEL, NOW),
        catalog_snapshots::publish(&pool, "a", CHANNEL, NOW),
    );
    let mut published = [first.unwrap(), second.unwrap()];
    published.sort_by_key(|snapshot| snapshot.version);
    let [first, second] = published;
    assert_eq!([first.version, second.version], [1, 2]);
    assert_eq!(first.snapshot_json, second.snapshot_json);
    assert_eq!(first.content_hash, second.content_hash);
    assert_eq!(
        second.content_hash,
        hex::encode(Sha256::digest(second.snapshot_json.as_bytes()))
    );
    let parsed: serde_json::Value = serde_json::from_str(&second.snapshot_json).unwrap();
    assert_eq!(parsed.to_string(), second.snapshot_json);
    let content = second.decode().unwrap();
    assert_eq!(content.categories.len(), 1);
    assert_eq!(content.categories[0].products.len(), 2);
    assert!(!content.product("a-soldout").unwrap().is_available);
    let visible = content.product("a-product").unwrap();
    assert_eq!(visible.variations, vec![variation.clone()]);
    assert_eq!(visible.addon_groups.len(), 1);
    assert_eq!(visible.addon_groups[0].options, vec![option()]);
    assert!(content.product("b-product").is_none());
    assert!(
        catalog_snapshots::current(&pool, "b", CHANNEL)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        catalog_snapshots::find_version(&pool, "b", CHANNEL, 2)
            .await
            .unwrap()
            .is_none()
    );

    variation.price_centavos = 9999;
    product_variations::save(&pool, "a", "a-product", &variation, NOW)
        .await
        .unwrap();
    assert!(
        update_product_for_est(
            &pool,
            "a",
            "a-product",
            "a-cat",
            "Segredo",
            "",
            "",
            "",
            "",
            "",
            9999,
            None,
            true,
            true,
            NOW
        )
        .await
        .unwrap()
    );
    assert_eq!(
        catalog_snapshots::current(&pool, "a", CHANNEL)
            .await
            .unwrap()
            .unwrap(),
        second
    );
    let derived = catalog_snapshots::publish_product_availability(
        &pool,
        "a",
        CHANNEL,
        "a-product",
        false,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(derived.version, 3);
    assert_ne!(derived.content_hash, second.content_hash);
    let derived_content = derived.decode().unwrap();
    let mut expected = visible.clone();
    expected.is_available = false;
    assert_eq!(derived_content.product("a-product"), Some(&expected));
    assert!(!derived.snapshot_json.contains("Segredo"));

    let historic = catalog_snapshots::find_version(&pool, "a", CHANNEL, 2)
        .await
        .unwrap()
        .unwrap();
    assert!(!historic.is_current);
    assert_eq!(historic.snapshot_json, second.snapshot_json);
    let update = sqlx::query("UPDATE catalog_snapshots SET snapshot_json = '{}' WHERE channel = ? AND establishment_id = ? AND version = ?")
        .bind(CHANNEL).bind("a").bind(2_i64).execute(&pool).await;
    assert!(update.is_err());
    sqlx::query("DELETE FROM establishments WHERE id = ?")
        .bind("a")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        catalog_snapshots::find_version(&pool, "a", CHANNEL, 2)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn republish_all_preserves_public_menu_bytes_across_commerce_migration_without_options() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    std::fs::create_dir_all(cfg.db_dir()).unwrap();
    let pool = open_pool_at(&cfg.db_path(), 4, cfg.db_synchronous)
        .await
        .unwrap();
    MIGRATOR.run_to(20261124000012, &pool).await.unwrap();
    seed(&pool).await;
    let locks = PublishLocks::default();
    // The pre-Commerce schema published from the base menu without catalog options.
    let snapshot = load_menu_snapshot(&pool, "a", false)
        .await
        .unwrap()
        .unwrap();
    write_static_snapshot(&cfg, &snapshot).await.unwrap();
    sqlx::query(
        "UPDATE establishments SET is_published = 1, published_version = 1,
         has_unpublished_changes = 0 WHERE id = 'a'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let index = cfg.published_dir().join("commerce-a/index.html");
    let before = std::fs::read(&index).unwrap();

    MIGRATOR.run(&pool).await.unwrap();
    catalog_snapshots::publish(&pool, "a", CHANNEL, NOW)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&index).unwrap(), before);
    assert_eq!(republish_all(&pool, &cfg, &locks).await.unwrap(), 1);
    assert_eq!(std::fs::read(&index).unwrap(), before);
}
