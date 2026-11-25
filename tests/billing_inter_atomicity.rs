//! A bank settlement and its local billing effects share one durable transaction.

#[path = "commercial/support.rs"]
mod support;

use chrono::Utc;
use uuid::Uuid;

use qrcardapio::application::billing_inter::{apply_snapshot, start};
use qrcardapio::config::{AsaasEnvironment, InterConfig};
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::domain::commercial::catalog;
use qrcardapio::domain::payer::{PayerInput, PayerProfile};
use qrcardapio::domain::payments::{PaymentMethod, PaymentProvider};
use qrcardapio::error::AppError;
use qrcardapio::infrastructure::database::begin_immediate;
use qrcardapio::infrastructure::database::queries::{
    billing::{self, CheckoutTarget},
    billing_inter, billing_payers,
};
use qrcardapio::infrastructure::inter::{ChargeSnapshot, InterClient};
use support::{ENV, harness, owner};

#[tokio::test]
async fn failed_settlement_keeps_inter_charge_eligible_for_reconciliation() {
    let fixture = harness(false).await;
    let pool = &fixture.pool;
    let (_, establishment) = owner(pool, "inter-atomic").await;
    let reference = Uuid::now_v7().to_string();
    let quote = catalog().entry_quote(BillingPlan::Monthly);
    let date = billing_now().date_naive();
    assert!(
        billing::reserve_contract_checkout(
            pool,
            ENV,
            &establishment,
            &reference,
            date,
            &quote,
            CheckoutTarget {
                provider: PaymentProvider::Inter,
                method: PaymentMethod::PixBoleto,
            },
        )
        .await
        .unwrap()
    );
    let mut tx = begin_immediate(pool).await.unwrap();
    let now = Utc::now().to_rfc3339();
    let number = "Q-ATOMIC-TEST";
    let request_code = Uuid::now_v7().to_string();
    billing_inter::reserve(
        &mut tx,
        ENV,
        &billing_inter::NewCharge {
            our_number: number,
            billing_reference: &reference,
            establishment_id: &establishment,
            cycle_index: 0,
            due_date: date,
            amount_centavos: quote.total_centavos,
            now: &now,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    billing_inter::attach_request(pool, ENV, number, &request_code, &now)
        .await
        .unwrap();

    sqlx::query(
        "CREATE TRIGGER fail_billing_write BEFORE INSERT ON billing_payments
         BEGIN SELECT RAISE(ABORT, 'simulated transient settlement failure'); END",
    )
    .execute(pool)
    .await
    .unwrap();
    let snapshot = ChargeSnapshot {
        request_code: request_code.clone(),
        our_number: Some(number.into()),
        status: "RECEBIDO".into(),
        due_date: Some(date),
        received_centavos: Some(quote.total_centavos),
        status_date: None,
        pix_copy_paste: None,
        digitable_line: None,
    };
    assert!(apply_snapshot(pool, ENV, &snapshot).await.is_err());
    let open = billing_inter::open_charges(pool, ENV, 200).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].request_code.as_deref(), Some(request_code.as_str()));
    assert_eq!(open[0].status, "issued");
    let checkout = billing::current_checkout(pool, ENV, &establishment)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkout.checkout_status, "open");

    sqlx::query("DROP TRIGGER fail_billing_write")
        .execute(pool)
        .await
        .unwrap();
    apply_snapshot(pool, ENV, &snapshot).await.unwrap();
    assert!(
        billing_inter::open_charges(pool, ENV, 200)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        billing::paid_through(pool, ENV, &establishment, date)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn failed_first_charge_reservation_rolls_back_checkout_and_contract() {
    let fixture = harness(false).await;
    let pool = &fixture.pool;
    let (login, establishment) = owner(pool, "inter-first-charge").await;
    let payer = PayerProfile::parse(PayerInput {
        document: "11.222.333/0001-81",
        legal_name: "Bistro da Praca Ltda",
        email: "financeiro@bistro.test",
        postal_code: "01310-100",
        street: "Avenida Paulista",
        number: "1000",
        complement: "",
        district: "Bela Vista",
        city: "Sao Paulo",
        state: "SP",
    })
    .unwrap();
    billing_payers::upsert(
        pool,
        &establishment,
        &payer,
        &login.account.id,
        &Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
    let client = InterClient::new(
        InterConfig::new(
            AsaasEnvironment::Sandbox,
            "client",
            "secret",
            Vec::new(),
            None,
            "interWebhookToken0123456789abcdefXYZ",
        )
        .unwrap()
        .with_loopback_origin("http://127.0.0.1:1")
        .unwrap(),
    )
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_first_cycle BEFORE INSERT ON billing_inter_charges
         BEGIN SELECT RAISE(ABORT, 'simulated first cycle reservation failure'); END",
    )
    .execute(pool)
    .await
    .unwrap();

    let quote = catalog().entry_quote(BillingPlan::Monthly);
    assert!(matches!(
        start(pool, &client, &establishment, &quote).await,
        Err(AppError::Database(_))
    ));
    assert!(
        billing::current_checkout(pool, ENV, &establishment)
            .await
            .unwrap()
            .is_none()
    );
    let contracts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM commercial_contracts WHERE environment = ? AND establishment_id = ?",
    )
    .bind(ENV)
    .bind(establishment)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(contracts, 0);
    assert!(
        billing_inter::open_charges(pool, ENV, 200)
            .await
            .unwrap()
            .is_empty()
    );
}
