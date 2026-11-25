//! Regression coverage for gateway changes received before their settlement.

#[path = "commercial/support.rs"]
mod support;

use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

use qrcardapio::application::affiliates::{
    AdminEnrollAffiliateInput, admin_enroll_affiliate, attribute_new_account,
};
use qrcardapio::application::billing::apply_events;
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::domain::commercial::catalog;
use qrcardapio::domain::payments::{
    GatewayEvent, PRIORITY_SETTLED, PaymentMethod, PaymentProvider, SubscriptionState,
};
use qrcardapio::infrastructure::database::queries::billing::{self, CheckoutTarget};
use support::{ENV, harness, owner};

const INVOICE: &str = "stripe:in_delayed_invoice";
const SUBSCRIPTION: &str = "stripe:sub_delayed_invoice";

async fn linked_checkout(pool: &SqlitePool, date: NaiveDate) -> (String, i64) {
    owner(pool, "stripe-partner").await;
    admin_enroll_affiliate(
        pool,
        AdminEnrollAffiliateInput {
            email: "stripe-partner@example.test",
            code: "stripe-partner",
            status: "active",
            commission_type: "fixed_monthly_centavos",
            commission_value: "10,00",
            hold_days: 0,
            pix_key: "",
            notes: "",
        },
    )
    .await
    .unwrap();
    let (customer, establishment) = owner(pool, "stripe-ordering").await;
    assert!(
        attribute_new_account(
            pool,
            Some("stripe-partner"),
            &customer.account.id,
            &customer.account.email,
        )
        .await
        .unwrap()
    );

    let reference = Uuid::now_v7().to_string();
    let quote = catalog().entry_quote(BillingPlan::Monthly);
    assert!(
        billing::reserve_contract_checkout(
            pool,
            ENV,
            &establishment,
            &reference,
            date,
            &quote,
            CheckoutTarget {
                provider: PaymentProvider::Stripe,
                method: PaymentMethod::Card,
            },
        )
        .await
        .unwrap()
    );
    let session = "stripe:cs_test_delayed_invoice";
    billing::finish_checkout(pool, ENV, &establishment, &reference, Some(session))
        .await
        .unwrap();
    apply_events(
        pool,
        ENV,
        Some("stripe:evt_link"),
        vec![GatewayEvent::Subscription {
            id: SUBSCRIPTION.into(),
            checkout: Some(session.into()),
            state: SubscriptionState::Active,
            at: format!("{date} 10:00:00"),
        }],
    )
    .await
    .unwrap();
    (establishment, quote.total_centavos)
}

#[tokio::test]
async fn reversal_before_settlement_prevents_coverage_commission_and_activation() {
    let fixture = harness(false).await;
    let pool = &fixture.pool;
    let date = billing_now().date_naive();
    let (establishment, amount) = linked_checkout(pool, date).await;
    // Stripe timestamps have second precision. A reversal wins a timestamp tie.
    let at = format!("{date} 10:01:00");
    let reversal = GatewayEvent::PaymentReversed {
        payment_id: INVOICE.into(),
        at: at.clone(),
    };
    apply_events(
        pool,
        ENV,
        Some("stripe:evt_reversal"),
        vec![reversal.clone()],
    )
    .await
    .unwrap();
    apply_events(
        pool,
        ENV,
        Some("stripe:evt_settled"),
        vec![GatewayEvent::Payment {
            id: INVOICE.into(),
            checkout: None,
            subscription_id: SUBSCRIPTION.into(),
            confirmed_centavos: Some(amount),
            due_date: date,
            priority: PRIORITY_SETTLED,
            at,
        }],
    )
    .await
    .unwrap();
    apply_events(pool, ENV, Some("stripe:evt_reversal"), vec![reversal])
        .await
        .unwrap();
    assert_eq!(
        billing::paid_through(pool, ENV, &establishment, date)
            .await
            .unwrap(),
        None
    );
    assert_eq!(effects(pool, &establishment).await, (0, 0));

    let restored = GatewayEvent::PaymentRestored {
        payment_id: INVOICE.into(),
        at: format!("{date} 10:02:00"),
    };
    for _ in 0..2 {
        apply_events(
            pool,
            ENV,
            Some("stripe:evt_restored"),
            vec![restored.clone()],
        )
        .await
        .unwrap();
    }
    assert!(
        billing::paid_through(pool, ENV, &establishment, date)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(effects(pool, &establishment).await, (1, 1));
}

#[tokio::test]
async fn newer_dispute_restore_is_retained_before_the_invoice_arrives() {
    let fixture = harness(false).await;
    let pool = &fixture.pool;
    let date = billing_now().date_naive();
    let (establishment, amount) = linked_checkout(pool, date).await;

    apply_events(
        pool,
        ENV,
        Some("stripe:evt_restore_first"),
        vec![GatewayEvent::PaymentRestored {
            payment_id: INVOICE.into(),
            at: format!("{date} 10:03:00"),
        }],
    )
    .await
    .unwrap();
    let older_reversal = GatewayEvent::PaymentReversed {
        payment_id: INVOICE.into(),
        at: format!("{date} 10:02:00"),
    };
    apply_events(
        pool,
        ENV,
        Some("stripe:evt_reversal_before_invoice"),
        vec![older_reversal.clone()],
    )
    .await
    .unwrap();
    apply_events(
        pool,
        ENV,
        Some("stripe:evt_paid_after_restore"),
        vec![GatewayEvent::Payment {
            id: INVOICE.into(),
            checkout: None,
            subscription_id: SUBSCRIPTION.into(),
            confirmed_centavos: Some(amount),
            due_date: date,
            priority: PRIORITY_SETTLED,
            at: format!("{date} 10:01:00"),
        }],
    )
    .await
    .unwrap();
    apply_events(
        pool,
        ENV,
        Some("stripe:evt_other_older_reversal"),
        vec![older_reversal],
    )
    .await
    .unwrap();

    assert!(
        billing::paid_through(pool, ENV, &establishment, date)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(effects(pool, &establishment).await, (1, 1));
}

async fn effects(pool: &SqlitePool, establishment: &str) -> (i64, i64) {
    let commissions = sqlx::query_scalar(
        "SELECT COUNT(*) FROM affiliate_commissions
         WHERE environment = ? AND payment_id = ?",
    )
    .bind(ENV)
    .bind(INVOICE)
    .fetch_one(pool)
    .await
    .unwrap();
    let activations = sqlx::query_scalar(
        "SELECT COUNT(*) FROM domain_events
         WHERE establishment_id = ? AND event_type = 'SubscriptionActivated'",
    )
    .bind(establishment)
    .fetch_one(pool)
    .await
    .unwrap();
    (commissions, activations)
}
