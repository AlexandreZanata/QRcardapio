use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product,
};
use qrcardapio::domain::commerce::catalog_options::{AddonGroup, AddonOption, ProductVariation};
use qrcardapio::domain::commerce::discount::Discount;
use qrcardapio::domain::commerce::order::{
    Fulfillment, NewOrder, OrderAddonSelection, OrderChannel, OrderItemSelection,
};
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::queries::{
    addon_groups, addon_options, catalog_snapshots, product_variations,
};

use crate::support::Harness;

pub const CATALOG_CHANNEL: &str = "delivery";
const NOW: &str = "2026-11-25T00:00:00Z";

pub fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

pub async fn catalog(harness: &Harness, est: &str, time: DateTime<Utc>) -> NewOrder {
    let pool = &harness.pool;
    let category = create_category(
        pool,
        est,
        CategoryInput {
            name: "Lanches",
            name_en: "Snacks",
            name_es: "Bocadillos",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    let product = create_product(
        pool,
        &AssetProcessor::new(harness.dir.path().join("media")),
        est,
        ProductInput {
            category_id: &category.id,
            name: "Sanduíche",
            name_en: "Sandwich",
            name_es: "Bocadillo",
            description: "",
            description_en: "",
            description_es: "",
            price_raw: "15,00",
            is_available: true,
            is_visible: true,
        },
        None,
    )
    .await
    .unwrap();
    let variation_id = format!("{est}-large");
    let option_id = format!("{est}-cheese");
    let group_id = format!("{est}-extras");
    product_variations::save(pool, est, &product.id, &variation(&variation_id), NOW)
        .await
        .unwrap();
    addon_groups::save(
        pool,
        est,
        &AddonGroup {
            id: group_id.clone(),
            name: "Extras".into(),
            name_en: "Extras".into(),
            name_es: "Complementos".into(),
            min_select: 1,
            max_select: 1,
            position: 0,
        },
        NOW,
    )
    .await
    .unwrap();
    addon_options::save(
        pool,
        est,
        &group_id,
        &AddonOption {
            id: option_id.clone(),
            name: "Queijo".into(),
            name_en: "Cheese".into(),
            name_es: "Queso".into(),
            price_centavos: 125,
            position: 0,
            max_quantity: 2,
            is_available: true,
        },
        NOW,
    )
    .await
    .unwrap();
    addon_groups::link_to_product(pool, est, &product.id, &group_id, 0, NOW)
        .await
        .unwrap();

    let snapshot = publish(pool, est).await;
    NewOrder {
        channel: OrderChannel::Delivery,
        fulfillment: Fulfillment::Delivery,
        catalog_channel: CATALOG_CHANNEL.into(),
        catalog_version: snapshot.version,
        items: vec![OrderItemSelection {
            product_id: product.id,
            variation_id: Some(variation_id),
            quantity: 2,
            addons: vec![OrderAddonSelection {
                option_id,
                quantity: 2,
            }],
        }],
        discount: Some(Discount::BasisPoints(500)),
        delivery_fee_centavos: 500,
        acceptance_deadline_at: time + Duration::minutes(20),
        event_context: Default::default(),
    }
}

pub fn variation(id: &str) -> ProductVariation {
    ProductVariation {
        id: id.to_owned(),
        name: "Grande".into(),
        name_en: "Large".into(),
        name_es: "Grande".into(),
        price_centavos: 2_000,
        position: 0,
        is_available: true,
    }
}

pub async fn publish(pool: &SqlitePool, est: &str) -> catalog_snapshots::CatalogSnapshot {
    catalog_snapshots::publish(pool, est, CATALOG_CHANNEL, NOW)
        .await
        .unwrap()
}
