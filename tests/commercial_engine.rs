//! Commercial Engine: integer pricing, quote validation, immutable contract snapshots, catalog
//! registry guards and affiliate-eligible revenue.

#[path = "commercial/support.rs"]
mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use qrcardapio::application::commercial::{parse_offer, quote_checkout};
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::domain::commercial::{
    BillingCycle, CommercialCatalog, OfferKind, OfferRef, QuoteError, QuoteRequest, SaleChannel,
    apply_discount, catalog,
};
use qrcardapio::infrastructure::database::queries::{billing, commercial_catalog};
use support::*;

fn offer(kind: OfferKind, code: &str) -> OfferRef {
    OfferRef {
        kind,
        code: code.to_owned(),
    }
}

#[test]
fn cycle_prices_are_identical_to_the_legacy_plan() {
    for (cycle, list, total) in [
        (BillingCycle::Monthly, 2_990, 2_990),
        (BillingCycle::Semiannual, 17_940, 17_043),
        (BillingCycle::Annual, 35_880, 32_292),
    ] {
        let quote = catalog().entry_quote(cycle);
        assert_eq!(
            (quote.list_total_centavos, quote.total_centavos),
            (list, total)
        );
        assert_eq!(quote.affiliate_eligible_centavos, total);
        assert_eq!(cycle.price(), total);
        assert_eq!(cycle.full_price(), list);
        assert_eq!(quote.lines.len(), 1);
        let line = &quote.lines[0];
        assert_eq!((line.code.as_str(), line.version), ("menu", 1));
        assert_eq!(line.discount_bps, cycle.discount_bps());
        assert_eq!(quote.capabilities().len(), 1);
    }
}

#[test]
fn discounts_use_deterministic_integer_rounding() {
    // Every bundle and module price on every cycle is reproducible from integer math only.
    for module in &catalog().modules {
        let monthly = module.current().monthly_price_centavos;
        for cycle in BillingCycle::ALL {
            let list = monthly * cycle.months();
            let expected = (list * (10_000 - cycle.discount_bps()) + 5_000) / 10_000;
            assert_eq!(apply_discount(list, cycle.discount_bps()), expected);
        }
    }
    assert_eq!(apply_discount(6_990 * 6, 500), 39_843);
    assert_eq!(apply_discount(6_990 * 12, 1_000), 75_492);
    assert_eq!(apply_discount(21_990 * 12, 1_000), 237_492);
}

#[test]
fn unavailable_duplicated_or_overlapping_offers_are_rejected() {
    let quote = |offers: Vec<OfferRef>, channel| {
        catalog().quote(
            &QuoteRequest {
                offers,
                cycle: BillingCycle::Monthly,
            },
            channel,
        )
    };
    assert_eq!(
        quote(vec![], SaleChannel::Standard).unwrap_err(),
        QuoteError::Empty
    );
    assert_eq!(
        quote(
            vec![offer(OfferKind::Module, "premium")],
            SaleChannel::Standard
        )
        .unwrap_err(),
        QuoteError::UnknownOffer
    );
    for module in catalog().modules.iter().filter(|m| m.code != "menu") {
        for channel in [SaleChannel::Standard, SaleChannel::BetaEnablement] {
            assert_eq!(
                quote(vec![offer(OfferKind::Module, &module.code)], channel).unwrap_err(),
                QuoteError::NotSellable,
                "{}",
                module.code
            );
        }
    }
    for bundle in &catalog().bundles {
        assert_eq!(
            quote(
                vec![offer(OfferKind::Bundle, &bundle.code)],
                SaleChannel::BetaEnablement
            )
            .unwrap_err(),
            QuoteError::NotSellable
        );
    }
    assert_eq!(
        quote(
            vec![offer(OfferKind::Addon, "assisted_onboarding")],
            SaleChannel::Standard
        )
        .unwrap_err(),
        QuoteError::NotSelfService
    );
    assert_eq!(
        quote(
            vec![
                offer(OfferKind::Module, "menu"),
                offer(OfferKind::Module, "menu")
            ],
            SaleChannel::Standard
        )
        .unwrap_err(),
        QuoteError::Duplicate
    );
    // Overlap is checked against a catalog where the bundle is sellable.
    let mut modules: Value =
        serde_json::from_str(include_str!("../contracts/commercial/modules.json")).unwrap();
    let mut bundles: Value =
        serde_json::from_str(include_str!("../contracts/commercial/bundles.json")).unwrap();
    modules["modules"][1]["lifecycle"] = "available".into();
    bundles["bundles"][0]["lifecycle"] = "available".into();
    let launched = CommercialCatalog::load(
        &modules.to_string(),
        &bundles.to_string(),
        include_str!("../contracts/commercial/addons.json"),
        include_str!("../contracts/commercial/billing-cycles.json"),
    )
    .unwrap();
    let request = |offers| QuoteRequest {
        offers,
        cycle: BillingCycle::Annual,
    };
    let start = launched
        .quote(
            &request(vec![offer(OfferKind::Bundle, "start")]),
            SaleChannel::Standard,
        )
        .unwrap();
    assert_eq!(start.total_centavos, 91_692);
    assert_eq!(
        start.capabilities().len(),
        5,
        "shared capabilities are not duplicated"
    );
    assert_eq!(
        launched
            .quote(
                &request(vec![
                    offer(OfferKind::Bundle, "start"),
                    offer(OfferKind::Module, "menu")
                ]),
                SaleChannel::Standard
            )
            .unwrap_err(),
        QuoteError::Overlap
    );
    let custom = launched
        .quote(
            &request(vec![
                offer(OfferKind::Module, "menu"),
                offer(OfferKind::Module, "delivery"),
            ]),
            SaleChannel::Standard,
        )
        .unwrap();
    assert_eq!(
        custom.total_centavos,
        32_292 + 75_492,
        "per-line rounding is summed"
    );
}

#[test]
fn submitted_offer_codes_cannot_carry_prices() {
    assert!(parse_offer("menu:2990").is_err());
    assert!(parse_offer("module:menu").is_ok());
    assert_eq!(
        quote_checkout(
            &[parse_offer("module:menu").unwrap()],
            BillingPlan::Semiannual
        )
        .unwrap()
        .total_centavos,
        17_043
    );
}

#[tokio::test]
async fn checkout_reservation_stores_an_immutable_contract_snapshot() {
    let h = harness(true).await;
    let (_, est) = owner(&h.pool, "engine-snapshot").await;
    let today = billing_now().date_naive();
    let reference = Uuid::now_v7().to_string();
    let quote = quote_checkout(&[parse_offer("menu").unwrap()], BillingPlan::Semiannual).unwrap();
    assert!(
        billing::reserve_contract_checkout(
            &h.pool,
            ENV,
            &est,
            &reference,
            today,
            &quote,
            billing::CheckoutTarget::ASAAS_CARD
        )
        .await
        .unwrap()
    );
    // A second reservation while the first is open never creates a second charge.
    assert!(
        !billing::reserve_contract_checkout(
            &h.pool,
            ENV,
            &est,
            &Uuid::now_v7().to_string(),
            today,
            &quote,
            billing::CheckoutTarget::ASAAS_CARD
        )
        .await
        .unwrap()
    );

    let contract: (String, String, i64, i64, i64, i64, i64, String) = sqlx::query_as(
        "SELECT origin, cycle_code, cycle_months, cycle_discount_bps, list_total_centavos, total_centavos,
                affiliate_eligible_centavos, billing_reference
         FROM commercial_contracts WHERE id = ? AND establishment_id = ?",
    )
    .bind(&reference)
    .bind(&est)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(
        contract,
        (
            "checkout".into(),
            "semiannual".into(),
            6,
            500,
            17_940,
            17_043,
            17_043,
            reference.clone()
        )
    );
    let gateway: (i64, i64) = sqlx::query_as(
        "SELECT plan_months, price_centavos FROM billing_checkouts WHERE reference = ?",
    )
    .bind(&reference)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(gateway, (6, 17_043));
    let item: (String, String, i64, i64, i64) = sqlx::query_as(
        "SELECT offer_kind, offer_code, offer_version, monthly_price_centavos, price_centavos
         FROM commercial_contract_items WHERE contract_id = ?",
    )
    .bind(&reference)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(item, ("module".into(), "menu".into(), 1, 2_990, 17_043));
    let caps: Vec<String> = sqlx::query_scalar(
        "SELECT capability FROM commercial_contract_capabilities WHERE contract_id = ?",
    )
    .bind(&reference)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(caps, ["menu.publish"]);
    // The database refuses snapshots whose discount exceeds the list price.
    let invalid = sqlx::query(
        "UPDATE commercial_contracts SET total_centavos = list_total_centavos + 1 WHERE id = ?",
    )
    .bind(&reference)
    .execute(&h.pool)
    .await;
    assert!(invalid.is_err());
}

fn catalog_with(edit: impl FnOnce(&mut Value)) -> CommercialCatalog {
    let mut modules: Value =
        serde_json::from_str(include_str!("../contracts/commercial/modules.json")).unwrap();
    edit(&mut modules);
    CommercialCatalog::load(
        &modules.to_string(),
        include_str!("../contracts/commercial/bundles.json"),
        include_str!("../contracts/commercial/addons.json"),
        include_str!("../contracts/commercial/billing-cycles.json"),
    )
    .unwrap()
}

#[tokio::test]
async fn sold_catalog_versions_and_lifecycles_cannot_change_silently() {
    let h = harness(false).await;
    let offers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commercial_catalog_offers")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(offers, 9 + 3 + 5);
    let frozen: Vec<(String, i64)> =
        sqlx::query_as("SELECT offer_code, version FROM commercial_catalog_versions")
            .fetch_all(&h.pool)
            .await
            .unwrap();
    assert_eq!(
        frozen,
        [("menu".to_owned(), 1)],
        "only sellable versions are frozen"
    );
    // Re-syncing the same catalog is idempotent.
    commercial_catalog::sync_registry(&h.pool, catalog())
        .await
        .unwrap();

    let repriced =
        catalog_with(|m| m["modules"][0]["versions"][0]["monthly_price_centavos"] = 3490.into());
    let error = commercial_catalog::sync_registry(&h.pool, &repriced)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("changed after it was sold"),
        "{error}"
    );
    let launched_early = catalog_with(|m| m["modules"][1]["lifecycle"] = "available".into());
    let error = commercial_catalog::sync_registry(&h.pool, &launched_early)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot move from planned to available"),
        "{error}"
    );
    // A planned price may still change before launch, and legal transitions are accepted.
    let refined = catalog_with(|m| {
        m["modules"][1]["versions"][0]["monthly_price_centavos"] = 7490.into();
        m["modules"][1]["lifecycle"] = "development".into();
    });
    commercial_catalog::sync_registry(&h.pool, &refined)
        .await
        .unwrap();
    let back = commercial_catalog::sync_registry(&h.pool, catalog()).await;
    assert!(back.is_ok(), "development -> planned is allowed");
}

#[tokio::test]
async fn registry_tolerates_rollbacks_but_not_new_sellable_offers() {
    let h = harness(false).await;
    // Walk QR Delivery through its legal launch path, one release at a time.
    let mut previous = "planned";
    for step in ["development", "internal_preview", "beta", "available"] {
        let release = catalog_with(|m| m["modules"][1]["lifecycle"] = step.into());
        commercial_catalog::sync_registry(&h.pool, &release)
            .await
            .unwrap_or_else(|e| panic!("{previous} -> {step}: {e}"));
        previous = step;
    }
    // The launch release fails its smoke checks: the previous binary (beta) must still start.
    let rolled_back = catalog_with(|m| m["modules"][1]["lifecycle"] = "beta".into());
    commercial_catalog::sync_registry(&h.pool, &rolled_back)
        .await
        .unwrap();
    // A sold version stays frozen across the rollback.
    let repriced = catalog_with(|m| {
        m["modules"][1]["lifecycle"] = "beta".into();
        m["modules"][1]["versions"][0]["monthly_price_centavos"] = 7990.into();
    });
    let error = commercial_catalog::sync_registry(&h.pool, &repriced)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("changed after it was sold"),
        "{error}"
    );

    // A brand-new offer cannot appear directly as sellable after the first start-up.
    let fresh = harness(false).await;
    let mut addons: Value =
        serde_json::from_str(include_str!("../contracts/commercial/addons.json")).unwrap();
    addons["addons"].as_array_mut().unwrap().push(json!({
        "code": "priority_support", "name": "Priority support", "lifecycle": "available",
        "pricing": "custom_quote", "affiliate_eligible": false, "current_version": null, "versions": []
    }));
    let surprise = CommercialCatalog::load(
        include_str!("../contracts/commercial/modules.json"),
        include_str!("../contracts/commercial/bundles.json"),
        &addons.to_string(),
        include_str!("../contracts/commercial/billing-cycles.json"),
    )
    .unwrap();
    let error = commercial_catalog::sync_registry(&fresh.pool, &surprise)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("must first ship as planned"),
        "{error}"
    );
}

#[tokio::test]
async fn affiliate_commissions_use_eligible_saas_revenue_once_per_establishment() {
    let h = harness(true).await;
    let (partner, _) = owner(&h.pool, "engine-partner").await;
    let (customer, est) = owner(&h.pool, "engine-referred").await;
    sqlx::query(
        "INSERT INTO affiliates (id, account_id, code, status, commission_type, commission_value,
            hold_days, created_at, updated_at)
         VALUES ('aff-1', ?, 'parceiro-teste', 'active', 'percent_bps', 2000, 0, datetime('now'), datetime('now'))",
    )
    .bind(&partner.account.id)
    .execute(&h.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO affiliate_referrals (id, affiliate_id, referred_account_id, status, attributed_at, updated_at)
         VALUES ('ref-1', 'aff-1', ?, 'signed_up', datetime('now'), datetime('now'))",
    )
    .bind(&customer.account.id)
    .execute(&h.pool)
    .await
    .unwrap();
    let today = billing_now().date_naive();
    let due = today.to_string();
    let reference = Uuid::now_v7().to_string();
    let checkout = Uuid::now_v7().to_string();
    assert!(
        billing::reserve_checkout(&h.pool, ENV, &est, &reference, today, BillingPlan::Monthly)
            .await
            .unwrap()
    );
    billing::finish_checkout(&h.pool, ENV, &est, &reference, Some(&checkout))
        .await
        .unwrap();
    // Simulate a contract whose add-on revenue (R$ 9,90) is not commission-eligible.
    sqlx::query("UPDATE commercial_contracts SET affiliate_eligible_centavos = 2000 WHERE id = ?")
        .bind(&reference)
        .execute(&h.pool)
        .await
        .unwrap();
    link_subscription(&h.app, &checkout, "sub_aff", &due, "aff").await;
    let paid = payment_event(
        "evt-aff",
        "PAYMENT_CONFIRMED",
        "pay_aff",
        "sub_aff",
        &due,
        &format!("{due} 10:00:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, paid).await, StatusCode::OK);
    let commission: (i64, i64) = sqlx::query_as(
        "SELECT SUM(commission_centavos), MAX(gross_payment_centavos) FROM affiliate_commissions
         WHERE establishment_id = ? AND entry_kind = 'commission'",
    )
    .bind(&est)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(
        commission,
        (400, 2_990),
        "20% of the eligible R$ 20,00, gross preserved"
    );
}
