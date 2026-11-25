use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::commerce::checkout;
use crate::domain::commerce::business_hours::ServiceMode;
use crate::domain::commerce::checkout::{
    CheckoutCart, CheckoutOutcome, CheckoutRequest, CheckoutTarget,
};
use crate::domain::commerce::customer::CustomerProfile;
use crate::domain::commerce::order::OrderItemSelection;
use crate::domain::commerce::order_payment::{OfflineMethod, OfflinePaymentChoice};
use crate::infrastructure::database::queries::catalog_snapshots;

pub(super) async fn checkout_fixture(pool: &SqlitePool) -> CheckoutCart {
    sqlx::raw_sql(
        "INSERT INTO commercial_settings (establishment_id, pickup) VALUES ('two', 1);
         INSERT INTO business_hours (establishment_id, weekday, windows_json)
            SELECT 'two', value, '[[0,1440]]' FROM json_each('[0,1,2,3,4,5,6]');
         INSERT INTO categories (id, establishment_id, name, position, created_at, updated_at)
            VALUES ('category', 'two', 'Category', 0, '2026-11-25', '2026-11-25');
         INSERT INTO products (id, establishment_id, category_id, name, price_centavos,
            position, created_at, updated_at)
            VALUES ('product', 'two', 'category', 'Product', 1000, 0, '2026-11-25', '2026-11-25');",
    )
    .execute(pool)
    .await
    .unwrap();
    let snapshot = catalog_snapshots::publish(pool, "two", "pickup", &Utc::now().to_rfc3339())
        .await
        .unwrap();
    CheckoutCart {
        catalog_version: snapshot.version,
        items: vec![OrderItemSelection {
            product_id: "product".into(),
            variation_id: None,
            quantity: 1,
            addons: Vec::new(),
        }],
    }
}

pub(super) async fn checkout_p95(pool: &SqlitePool, cart: &CheckoutCart) -> f64 {
    let target = CheckoutTarget {
        establishment_id: "two",
        catalog_channel: "pickup",
        mode: ServiceMode::Pickup,
        delivery_fee_centavos: 0,
        discount: None,
    };
    let mut schedule = tokio::time::interval(Duration::from_millis(25));
    let mut samples = Vec::with_capacity(600);
    for _ in 0..600 {
        schedule.tick().await;
        let request = CheckoutRequest {
            idempotency_key: Uuid::new_v4(),
            cart: cart.clone(),
            expected_total_centavos: 1000,
            customer: CustomerProfile {
                name: "Benchmark".into(),
                phone_e164: "+5565999990001".into(),
                email: String::new(),
            },
            address: None,
            payment: OfflinePaymentChoice {
                method: OfflineMethod::Cash,
                change_for_centavos: None,
            },
        };
        let start = Instant::now();
        assert!(matches!(
            checkout::place_order(pool, target, request, Utc::now())
                .await
                .unwrap(),
            CheckoutOutcome::Placed(_)
        ));
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    samples[569].as_secs_f64() * 1000.0
}
