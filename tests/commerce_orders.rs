#[path = "commerce/order_fixtures.rs"]
mod fixture;
#[path = "commercial/support.rs"]
mod support;

use chrono::{Duration, NaiveDate};
use serde_json::{Value, json};
use uuid::Uuid;

use qrcardapio::application::commerce::order_creation::create;
use qrcardapio::domain::commerce::order::{OrderEventContext, business_date};
use qrcardapio::domain::commerce::pricing::MAX_ORDER_TOTAL_CENTAVOS;
use qrcardapio::domain::commerce::state_machine::{Machine, State, definition};
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::queries::{
    catalog_snapshots, domain_events, orders, product_variations,
};

#[tokio::test]
async fn order_copies_localized_published_prices_and_names_with_immutable_item_rows() {
    let harness = support::harness(false).await;
    let default_state: String = sqlx::query_scalar(
        "SELECT dflt_value FROM pragma_table_info('orders') WHERE name = 'state'",
    )
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        default_state,
        format!("'{}'", definition(Machine::Order).initial.as_str())
    );
    let (_, establishment) = support::owner(&harness.pool, "order-snapshot").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let mut command = fixture::catalog(&harness, &establishment, now).await;
    let line = &command.items[0];
    let correlation_id = Uuid::new_v4();
    let causation_id = Uuid::new_v4();
    command.event_context = OrderEventContext {
        correlation_id: Some(correlation_id),
        causation_id: Some(causation_id),
    };

    let mut variation = fixture::variation(line.variation_id.as_deref().unwrap());
    variation.price_centavos = 2_600;
    variation.name = "Rascunho".into();
    product_variations::save(
        &harness.pool,
        &establishment,
        &line.product_id,
        &variation,
        &now.to_rfc3339(),
    )
    .await
    .unwrap();
    let order = create(&harness.pool, &establishment, &command, now)
        .await
        .unwrap();
    assert_eq!(order.items_subtotal_centavos, 4_500);
    assert_eq!(order.discount_centavos, 225);
    assert_eq!(order.delivery_fee_centavos, 500);
    assert_eq!(order.total_centavos, 4_775);
    assert_eq!(State::from(order.state), definition(Machine::Order).initial);
    assert_eq!(order.public_id.len(), 32);
    assert_ne!(order.public_id, order.id);
    assert_eq!(
        orders::get(&harness.pool, &establishment, &order.id)
            .await
            .unwrap(),
        Some(order.clone())
    );

    let version = fixture::publish(&harness.pool, &establishment)
        .await
        .version;
    assert_eq!(version, order.catalog_version + 1);
    assert!(matches!(
        create(&harness.pool, &establishment, &command, now).await,
        Err(AppError::Conflict("commerce.checkout_repriced"))
    ));

    let item: (
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT name, name_en, name_es, variation_name, variation_name_en, variation_name_es,
                base_price_centavos, unit_price_centavos, line_total_centavos
         FROM order_items WHERE order_id = ? AND establishment_id = ?",
    )
    .bind(&order.id)
    .bind(&establishment)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        item,
        (
            "Sanduíche".into(),
            "Sandwich".into(),
            "Bocadillo".into(),
            "Grande".into(),
            "Large".into(),
            "Grande".into(),
            2_000,
            2_250,
            4_500,
        )
    );
    let addon: (String, String, String, String, i64, i64) = sqlx::query_as(
        "SELECT o.group_name_es, o.name, o.name_en, o.name_es, o.unit_price_centavos, o.quantity
         FROM order_item_options o JOIN order_items i
           ON i.id = o.order_item_id AND i.establishment_id = o.establishment_id
         WHERE i.order_id = ? AND o.establishment_id = ?",
    )
    .bind(&order.id)
    .bind(&establishment)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        addon,
        (
            "Complementos".into(),
            "Queijo".into(),
            "Cheese".into(),
            "Queso".into(),
            125,
            2
        )
    );
    for sql in [
        "UPDATE order_items SET name = 'changed' WHERE establishment_id = ?",
        "DELETE FROM order_items WHERE establishment_id = ?",
        "UPDATE order_item_options SET quantity = 1 WHERE establishment_id = ?",
        "DELETE FROM order_item_options WHERE establishment_id = ?",
    ] {
        assert!(
            sqlx::query(sql)
                .bind(&establishment)
                .execute(&harness.pool)
                .await
                .is_err(),
            "{sql}"
        );
    }

    let events = domain_events::list_for_establishment(&harness.pool, &establishment)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.event_type, "OrderCreated");
    assert_eq!(event.aggregate_id, order.id);
    assert_eq!(event.correlation_id, correlation_id.to_string());
    assert_eq!(
        event.causation_id.as_deref(),
        Some(causation_id.to_string().as_str())
    );
    assert_eq!(
        serde_json::from_str::<Value>(&event.payload).unwrap(),
        json!({
            "order_id": order.id,
            "channel": "delivery",
            "total_centavos": 4_775,
            "item_count": 1,
            "catalog_version": 1,
        })
    );
}

#[tokio::test]
async fn order_creation_guards_tenant_snapshot_availability_and_option_limits() {
    let harness = support::harness(false).await;
    let (_, alice) = support::owner(&harness.pool, "order-alice").await;
    let (_, bob) = support::owner(&harness.pool, "order-bob").await;
    let now = fixture::at("2026-11-25T15:00:00Z");
    let command = fixture::catalog(&harness, &alice, now).await;
    assert!(matches!(
        create(&harness.pool, &bob, &command, now).await,
        Err(AppError::Conflict("commerce.checkout_repriced"))
    ));

    let bob_catalog = fixture::catalog(&harness, &bob, now).await;
    let mut bad_variation = command.clone();
    bad_variation.items[0].variation_id = bob_catalog.items[0].variation_id.clone();
    let mut bad_addon = command.clone();
    bad_addon.items[0].addons = bob_catalog.items[0].addons.clone();
    let mut requires_variation = command.clone();
    requires_variation.items[0].variation_id = None;
    let mut requires_addon = command.clone();
    requires_addon.items[0].addons.clear();
    let mut excessive_addon_quantity = command.clone();
    excessive_addon_quantity.items[0].addons[0].quantity = 3;
    let mut duplicate_addon = command.clone();
    duplicate_addon.items[0]
        .addons
        .push(command.items[0].addons[0].clone());
    for command in [
        bad_variation,
        bad_addon,
        requires_variation,
        requires_addon,
        excessive_addon_quantity,
        duplicate_addon,
        bob_catalog,
    ] {
        assert!(matches!(
            create(&harness.pool, &alice, &command, now).await,
            Err(AppError::Conflict("commerce.checkout_repriced"))
        ));
    }

    let mut capped = command.clone();
    capped.delivery_fee_centavos = MAX_ORDER_TOTAL_CENTAVOS;
    assert!(matches!(
        create(&harness.pool, &alice, &capped, now).await,
        Err(AppError::Validation("commerce.invalid_order_price"))
    ));

    let current = catalog_snapshots::publish_product_availability(
        &harness.pool,
        &alice,
        fixture::CATALOG_CHANNEL,
        &command.items[0].product_id,
        false,
        &now.to_rfc3339(),
    )
    .await
    .unwrap();
    let mut unavailable = command;
    unavailable.catalog_version = current.version;
    assert!(matches!(
        create(&harness.pool, &alice, &unavailable, now).await,
        Err(AppError::Conflict("commerce.checkout_repriced"))
    ));
    let num_orders: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE establishment_id = ?")
            .bind(&alice)
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    assert_eq!(num_orders, 0);
    assert!(
        domain_events::list_for_establishment(&harness.pool, &alice)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn order_numbers_increment_per_establishment_and_sao_paulo_business_date() {
    let harness = support::harness(false).await;
    let (_, alice) = support::owner(&harness.pool, "order-number-alice").await;
    let (_, bob) = support::owner(&harness.pool, "order-number-bob").await;

    let before_midnight = fixture::at("2026-11-25T02:59:59Z");
    let midnight = fixture::at("2026-11-25T03:00:00Z");
    let mut command = fixture::catalog(&harness, &alice, before_midnight).await;
    let (first, second) = tokio::join!(
        create(&harness.pool, &alice, &command, before_midnight),
        create(&harness.pool, &alice, &command, before_midnight)
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    let mut numbers = [first.order_number, second.order_number];
    numbers.sort_unstable();
    assert_eq!(numbers, [1, 2]);
    assert_eq!(
        first.business_date,
        NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
    );
    assert_eq!(first.business_date, second.business_date);

    command.acceptance_deadline_at = midnight + Duration::minutes(20);
    let tomorrow = create(&harness.pool, &alice, &command, midnight)
        .await
        .unwrap();
    assert_eq!(tomorrow.order_number, 1);
    assert_eq!(
        tomorrow.business_date,
        NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
    );

    let bob_command = fixture::catalog(&harness, &bob, before_midnight).await;
    let bob_order = create(&harness.pool, &bob, &bob_command, before_midnight)
        .await
        .unwrap();
    assert_eq!(bob_order.order_number, 1);
    assert_eq!(bob_order.business_date, first.business_date);
    assert!(
        orders::get(&harness.pool, &bob, &first.id)
            .await
            .unwrap()
            .is_none()
    );

    // São Paulo was UTC−02 during this historic summer midnight.
    let historical = fixture::at("2018-01-02T02:30:00Z");
    assert_eq!(
        business_date(historical),
        NaiveDate::from_ymd_opt(2018, 1, 2)
    );
}
