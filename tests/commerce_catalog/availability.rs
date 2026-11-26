use qrcardapio::application::commerce::checkout::quote;
use qrcardapio::domain::commerce::business_hours::ServiceMode;
use qrcardapio::domain::commerce::catalog_options::{AddonOption, ProductVariation};
use qrcardapio::domain::commerce::catalog_snapshot::AvailabilityTarget;
use qrcardapio::domain::commerce::checkout::{CheckoutCart, CheckoutTarget};
use qrcardapio::domain::commerce::order::OrderItemSelection;
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::database::queries::{
    addon_groups, addon_options, catalog_snapshots, product_variations,
};
use tempfile::TempDir;

use super::CHANNEL;
use super::fixtures::{NOW, config, group, option, seed, variation};

#[tokio::test]
async fn option_availability_derives_a_scoped_snapshot_without_including_draft_edits() {
    let dir = TempDir::new().unwrap();
    let pool = init_pool(&config(&dir)).await.unwrap();
    seed(&pool).await;
    let variation = variation();
    let option = option();
    product_variations::save(&pool, "a", "a-product", &variation, NOW)
        .await
        .unwrap();
    addon_groups::save(&pool, "a", &group("shared"), NOW)
        .await
        .unwrap();
    addon_options::save(&pool, "a", "shared", &option, NOW)
        .await
        .unwrap();
    for product_id in ["a-product", "a-soldout"] {
        addon_groups::link_to_product(&pool, "a", product_id, "shared", 0, NOW)
            .await
            .unwrap();
    }
    let original = catalog_snapshots::publish(&pool, "a", CHANNEL, NOW)
        .await
        .unwrap();
    let other_channel = catalog_snapshots::publish(&pool, "a", "pickup", NOW)
        .await
        .unwrap();
    let other_tenant = catalog_snapshots::publish(&pool, "b", CHANNEL, NOW)
        .await
        .unwrap();

    let draft_variation = ProductVariation {
        name: "Alteração de rascunho".into(),
        price_centavos: 9_999,
        ..variation.clone()
    };
    let new_variation = ProductVariation {
        id: "draft-only".into(),
        ..draft_variation.clone()
    };
    let draft_option = AddonOption {
        name: "Adicional de rascunho".into(),
        price_centavos: 1_999,
        ..option.clone()
    };
    for draft in [&draft_variation, &new_variation] {
        product_variations::save(&pool, "a", "a-product", draft, NOW)
            .await
            .unwrap();
    }
    addon_options::save(&pool, "a", "shared", &draft_option, NOW)
        .await
        .unwrap();

    let variation_target = AvailabilityTarget::Variation {
        product_id: "a-product",
        variation_id: &variation.id,
    };
    let addon_target = AvailabilityTarget::AddonOption {
        group_id: "shared",
        option_id: &option.id,
    };
    let (variation_update, repeat) = tokio::join!(
        catalog_snapshots::publish_availability(&pool, "a", CHANNEL, variation_target, false, NOW),
        catalog_snapshots::publish_availability(&pool, "a", CHANNEL, variation_target, false, NOW),
    );
    let variation_update = variation_update.unwrap();
    assert_eq!(variation_update, repeat.unwrap());
    assert_eq!(variation_update.version, original.version + 1);

    let mut expected = original.decode().unwrap();
    let products = &mut expected.categories[0].products;
    products[0].variations[0].is_available = false;
    assert_eq!(variation_update.decode().unwrap(), expected);

    let addon_update =
        catalog_snapshots::publish_availability(&pool, "a", CHANNEL, addon_target, false, NOW)
            .await
            .unwrap();
    assert_eq!(addon_update.version, variation_update.version + 1);
    for product in &mut expected.categories[0].products {
        product.addon_groups[0].options[0].is_available = false;
    }
    assert_eq!(addon_update.decode().unwrap(), expected);
    assert_eq!(
        catalog_snapshots::publish_availability(&pool, "a", CHANNEL, addon_target, false, NOW)
            .await
            .unwrap(),
        addon_update
    );

    let wrong_product = AvailabilityTarget::Variation {
        product_id: "a-soldout",
        variation_id: &variation.id,
    };
    let wrong_group = AvailabilityTarget::AddonOption {
        group_id: "unknown-group",
        option_id: &option.id,
    };
    let only_in_draft = AvailabilityTarget::Variation {
        product_id: "a-product",
        variation_id: &new_variation.id,
    };
    for (tenant, channel, target) in [
        ("a", CHANNEL, wrong_product),
        ("a", CHANNEL, wrong_group),
        ("a", CHANNEL, only_in_draft),
        ("b", CHANNEL, variation_target),
        ("b", CHANNEL, addon_target),
        ("a", "unknown-channel", variation_target),
    ] {
        assert!(matches!(
            catalog_snapshots::publish_availability(&pool, tenant, channel, target, true, NOW)
                .await,
            Err(AppError::NotFound("error.not_found"))
        ));
    }
    for (tenant, channel, snapshot) in [
        ("a", CHANNEL, addon_update),
        ("a", "pickup", other_channel),
        ("b", CHANNEL, other_tenant),
    ] {
        assert_eq!(
            catalog_snapshots::current(&pool, tenant, channel)
                .await
                .unwrap(),
            Some(snapshot)
        );
    }
    for previous in [original, variation_update] {
        let stored = catalog_snapshots::find_version(&pool, "a", CHANNEL, previous.version)
            .await
            .unwrap()
            .unwrap();
        assert!(!stored.is_current);
        assert_eq!(stored.snapshot_json, previous.snapshot_json);
        assert_eq!(stored.content_hash, previous.content_hash);
    }
    assert_eq!(
        addon_options::list_for_group(&pool, "a", "shared")
            .await
            .unwrap(),
        [draft_option]
    );
    assert_eq!(
        product_variations::list_for_product(&pool, "a", "a-product")
            .await
            .unwrap(),
        [new_variation, draft_variation]
    );
}

#[tokio::test]
async fn quoting_uses_the_base_price_when_the_last_active_variation_is_unavailable() {
    let dir = TempDir::new().unwrap();
    let pool = init_pool(&config(&dir)).await.unwrap();
    seed(&pool).await;
    let variation = variation();
    product_variations::save(&pool, "a", "a-product", &variation, NOW)
        .await
        .unwrap();
    let initial = catalog_snapshots::publish(&pool, "a", CHANNEL, NOW)
        .await
        .unwrap();
    let target = CheckoutTarget {
        establishment_id: "a",
        catalog_channel: CHANNEL,
        mode: ServiceMode::Pickup,
        delivery_fee_centavos: 0,
        discount: None,
    };
    let now = NOW.parse().unwrap();
    let mut cart = CheckoutCart {
        catalog_version: initial.version,
        items: vec![OrderItemSelection {
            product_id: "a-product".into(),
            variation_id: None,
            quantity: 1,
            addons: vec![],
        }],
    };
    assert!(
        quote(&pool, target, &cart, now)
            .await
            .unwrap()
            .totals
            .is_none()
    );
    cart.items[0].variation_id = Some(variation.id.clone());
    assert_eq!(
        quote(&pool, target, &cart, now)
            .await
            .unwrap()
            .totals
            .unwrap()
            .total_centavos,
        variation.price_centavos
    );

    let current = catalog_snapshots::publish_availability(
        &pool,
        "a",
        CHANNEL,
        AvailabilityTarget::Variation {
            product_id: "a-product",
            variation_id: &variation.id,
        },
        false,
        NOW,
    )
    .await
    .unwrap();
    let unavailable = quote(&pool, target, &cart, now).await.unwrap();
    assert_eq!(unavailable.catalog_version, current.version);
    assert!(unavailable.totals.is_none());
    cart.catalog_version = current.version;
    cart.items[0].variation_id = None;
    assert_eq!(
        quote(&pool, target, &cart, now)
            .await
            .unwrap()
            .totals
            .unwrap()
            .total_centavos,
        1_500,
    );
}
