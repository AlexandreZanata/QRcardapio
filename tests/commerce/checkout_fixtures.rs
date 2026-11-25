use qrcardapio::domain::commerce::business_hours::{CommercialSettings, ServiceMode, Windows};
use qrcardapio::domain::commerce::checkout::{
    CheckoutCart, CheckoutOutcome, CheckoutReceipt, CheckoutRequest, CheckoutTarget,
};
use qrcardapio::domain::commerce::customer::{CustomerProfile, PostalAddress};
use qrcardapio::domain::commerce::discount::Discount;
use qrcardapio::domain::commerce::order_payment::{OfflineMethod, OfflinePaymentChoice};
use qrcardapio::infrastructure::database::queries::business_hours;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::fixture::CATALOG_CHANNEL;

pub fn target(establishment_id: &str) -> CheckoutTarget<'_> {
    CheckoutTarget {
        establishment_id,
        catalog_channel: CATALOG_CHANNEL,
        mode: ServiceMode::Delivery,
        discount: Some(Discount::BasisPoints(500)),
        delivery_fee_centavos: 500,
    }
}

pub fn request(key: Uuid, cart: &CheckoutCart) -> CheckoutRequest {
    CheckoutRequest {
        idempotency_key: key,
        cart: cart.clone(),
        expected_total_centavos: 4_775,
        customer: CustomerProfile {
            name: " Cliente Checkout ".into(),
            phone_e164: "(65) 99900-1234".into(),
            email: String::new(),
        },
        address: Some(PostalAddress {
            street: "Rua Particular".into(),
            street_number: "42".into(),
            complement: String::new(),
            neighborhood: "Centro".into(),
            city: "Cuiabá".into(),
            uf: "mt".into(),
            cep: "78000-999".into(),
            reference: String::new(),
        }),
        payment: OfflinePaymentChoice {
            method: OfflineMethod::Cash,
            change_for_centavos: Some(5_000),
        },
    }
}

pub async fn enable(pool: &SqlitePool, establishment: &str) -> CommercialSettings {
    let settings = CommercialSettings {
        time_zone: "America/Cuiaba".into(),
        delivery: true,
        pickup: true,
        minimum_centavos: 4_500,
        acceptance_minutes: 12,
        ..Default::default()
    };
    business_hours::save_settings(pool, establishment, &settings)
        .await
        .unwrap();
    let week = std::array::from_fn(|_| "00:00-24:00".parse::<Windows>().unwrap());
    business_hours::save_weekly(pool, establishment, &week)
        .await
        .unwrap();
    settings
}

pub fn placed(outcome: CheckoutOutcome) -> CheckoutReceipt {
    let CheckoutOutcome::Placed(receipt) = outcome else {
        panic!("expected placed checkout, got {outcome:?}");
    };
    receipt
}

pub async fn assert_counts(pool: &SqlitePool, expected: i64) {
    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT 'orders', COUNT(*) FROM orders
         UNION ALL SELECT 'order_items', COUNT(*) FROM order_items
         UNION ALL SELECT 'order_item_options', COUNT(*) FROM order_item_options
         UNION ALL SELECT 'checkout_attempts', COUNT(*) FROM checkout_attempts
         UNION ALL SELECT 'customers', COUNT(*) FROM customers
         UNION ALL SELECT 'order_customers', COUNT(*) FROM order_customers
         UNION ALL SELECT 'order_delivery_details', COUNT(*) FROM order_delivery_details
         UNION ALL SELECT 'order_delivery_neighborhoods', COUNT(*) FROM order_delivery_neighborhoods
         UNION ALL SELECT 'order_payments', COUNT(*) FROM order_payments
         UNION ALL SELECT 'domain_events', COUNT(*) FROM domain_events",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for (table, count) in counts {
        assert_eq!(count, expected, "{table}");
    }
}
