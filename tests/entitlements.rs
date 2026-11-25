//! Entitlement Engine and SaaS billing: access follows confirmed payments, never plan names.

#[path = "commercial/support.rs"]
mod support;

use axum::http::StatusCode;
use chrono::{Duration, NaiveDate};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use qrcardapio::application::entitlements::{Denial, Enforcement, authorize, resolve_capability};
use qrcardapio::application::operator::{self, GrantRequest};
use qrcardapio::domain::billing::{BillingPlan, billing_now};
use qrcardapio::domain::entitlements::{
    AccessSource, Capability, EntitlementStatus, GrantKind, MemberRole, Operation,
    OperationalStatus,
};
use qrcardapio::infrastructure::database::queries::billing;
use support::*;

const ENFORCED: Enforcement = Enforcement::Enforced { environment: ENV };

async fn open_checkout(
    pool: &SqlitePool,
    est: &str,
    plan: BillingPlan,
    today: NaiveDate,
) -> (String, String) {
    let reference = Uuid::now_v7().to_string();
    let checkout = Uuid::now_v7().to_string();
    assert!(
        billing::reserve_checkout(pool, ENV, est, &reference, today, plan)
            .await
            .unwrap()
    );
    billing::finish_checkout(pool, ENV, est, &reference, Some(&checkout))
        .await
        .unwrap();
    (reference, checkout)
}

async fn menu(
    pool: &SqlitePool,
    est: &str,
    day: NaiveDate,
) -> qrcardapio::domain::entitlements::EntitlementDecision {
    resolve_capability(pool, ENFORCED, est, Capability::MenuPublish, day)
        .await
        .unwrap()
}

async fn count_events(pool: &SqlitePool, est: &str, kind: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM domain_events WHERE establishment_id = ? AND event_type = ?",
    )
    .bind(est)
    .bind(kind)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn access_is_granted_by_confirmed_payment_and_ends_with_the_paid_period() {
    let h = harness(true).await;
    let (_, est) = owner(&h.pool, "ent-paid").await;
    let today = billing_now().date_naive();
    let due = today.to_string();
    let (reference, checkout) = open_checkout(&h.pool, &est, BillingPlan::Monthly, today).await;

    assert_eq!(
        menu(&h.pool, &est, today).await.status,
        EntitlementStatus::NotContracted
    );
    let denied = authorize(
        &h.pool,
        ENFORCED,
        &est,
        MemberRole::Owner,
        Operation::PublishMenu,
        today,
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(matches!(denied, Denial::NotEntitled(_)));

    link_subscription(&h.app, &checkout, "sub_paid", &due, "paid").await;
    // Partial payment never grants access.
    let partial = payment_event(
        "evt-partial",
        "PAYMENT_CONFIRMED",
        "pay_partial",
        "sub_paid",
        &due,
        &format!("{due} 10:00:00"),
        json!(10.00),
    );
    assert_eq!(webhook(&h.app, partial).await, StatusCode::OK);
    assert!(!menu(&h.pool, &est, today).await.is_allowed());

    let confirmed = payment_event(
        "evt-paid",
        "PAYMENT_CONFIRMED",
        "pay_paid",
        "sub_paid",
        &due,
        &format!("{due} 10:01:00"),
        json!(29.90),
    );
    let (a, b) = tokio::join!(
        webhook(&h.app, confirmed.clone()),
        webhook(&h.app, confirmed)
    );
    assert_eq!((a, b), (StatusCode::OK, StatusCode::OK));
    let decision = menu(&h.pool, &est, today).await;
    assert_eq!(decision.status, EntitlementStatus::Active);
    assert_eq!(
        decision.source,
        Some(AccessSource::Contract {
            contract_id: reference.clone()
        })
    );
    let ends = decision.ends_on.unwrap();
    assert_eq!(decision.starts_on, Some(today));
    assert_eq!(
        Some(ends),
        billing::paid_through(&h.pool, ENV, &est, today)
            .await
            .unwrap()
    );
    assert!(
        menu(&h.pool, &est, ends - Duration::days(1))
            .await
            .is_allowed()
    );
    assert_eq!(
        menu(&h.pool, &est, ends).await.status,
        EntitlementStatus::Expired
    );
    assert_eq!(
        count_events(&h.pool, &est, "SubscriptionActivated").await,
        1
    );
    let processed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM billing_webhook_events WHERE event_id = 'evt-paid'",
    )
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(processed, 1);

    // Refund revokes; an older out-of-order settlement event cannot restore access.
    let refund = payment_event(
        "evt-refund",
        "PAYMENT_REFUNDED",
        "pay_paid",
        "sub_paid",
        &due,
        &format!("{due} 10:05:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, refund).await, StatusCode::OK);
    let late = payment_event(
        "evt-late",
        "PAYMENT_RECEIVED",
        "pay_paid",
        "sub_paid",
        &due,
        &format!("{due} 10:03:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, late).await, StatusCode::OK);
    assert!(!menu(&h.pool, &est, today).await.is_allowed());
    assert_eq!(
        count_events(&h.pool, &est, "SubscriptionActivated").await,
        1
    );
}

#[tokio::test]
async fn chargebacks_revoke_and_cancellation_keeps_the_paid_period() {
    let h = harness(true).await;
    let today = billing_now().date_naive();
    let due = today.to_string();

    let (_, kept) = owner(&h.pool, "ent-cancelled").await;
    let (_, checkout) = open_checkout(&h.pool, &kept, BillingPlan::Annual, today).await;
    link_subscription(&h.app, &checkout, "sub_cancel", &due, "cancel").await;
    let paid = payment_event(
        "evt-annual",
        "PAYMENT_CONFIRMED",
        "pay_annual",
        "sub_cancel",
        &due,
        &format!("{due} 10:00:00"),
        json!(322.92),
    );
    assert_eq!(webhook(&h.app, paid).await, StatusCode::OK);
    let deleted = json!({"id": "evt-deleted", "event": "SUBSCRIPTION_DELETED", "dateCreated": format!("{due} 11:00:00"), "subscription": {"id": "sub_cancel"}});
    assert_eq!(webhook(&h.app, deleted).await, StatusCode::OK);
    let decision = menu(&h.pool, &kept, today + Duration::days(200)).await;
    assert_eq!(decision.status, EntitlementStatus::Active);
    assert_eq!(
        decision.ends_on,
        Some(
            today
                .checked_add_months(chrono::Months::new(12))
                .unwrap_or(today)
        )
    );

    let (_, disputed) = owner(&h.pool, "ent-chargeback").await;
    let (_, checkout) = open_checkout(&h.pool, &disputed, BillingPlan::Monthly, today).await;
    link_subscription(&h.app, &checkout, "sub_dispute", &due, "dispute").await;
    let paid = payment_event(
        "evt-d-paid",
        "PAYMENT_CONFIRMED",
        "pay_dispute",
        "sub_dispute",
        &due,
        &format!("{due} 10:00:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, paid).await, StatusCode::OK);
    assert!(menu(&h.pool, &disputed, today).await.is_allowed());
    let chargeback = payment_event(
        "evt-d-cb",
        "PAYMENT_CHARGEBACK_REQUESTED",
        "pay_dispute",
        "sub_dispute",
        &due,
        &format!("{due} 12:00:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, chargeback).await, StatusCode::OK);
    assert!(!menu(&h.pool, &disputed, today).await.is_allowed());
    // Isolation: neither establishment sees the other's coverage or the production environment.
    let prod = Enforcement::Enforced {
        environment: "production",
    };
    assert!(
        !resolve_capability(&h.pool, prod, &kept, Capability::MenuPublish, today)
            .await
            .unwrap()
            .is_allowed()
    );
}

#[tokio::test]
async fn grants_are_an_independent_audited_source() {
    let h = harness(true).await;
    admin(&h.pool, "ops@example.test").await;
    let (_, est) = owner(&h.pool, "ent-grants").await;
    let (_, other) = owner(&h.pool, "ent-grants-other").await;
    let today = billing_now().date_naive();
    let due = today.to_string();
    let request =
        |offer: &'static str, kind: GrantKind, days: i64, slug: &'static str| GrantRequest {
            operator_email: "ops@example.test",
            establishment_slug: slug,
            offer_code: offer,
            kind,
            starts_on: today,
            ends_on: today + Duration::days(days),
            reason: "pilot customer",
        };

    let grant = operator::grant(
        &h.pool,
        request("menu", GrantKind::Courtesy, 30, "ent-grants"),
    )
    .await
    .unwrap();
    let decision = menu(&h.pool, &est, today).await;
    assert!(matches!(
        decision.source,
        Some(AccessSource::Grant {
            kind: GrantKind::Courtesy,
            ..
        })
    ));
    assert!(
        !menu(&h.pool, &other, today).await.is_allowed(),
        "grants are tenant-scoped"
    );

    // Add a paid contract, then remove the grant: the contract keeps the capability.
    let (reference, checkout) = open_checkout(&h.pool, &est, BillingPlan::Monthly, today).await;
    link_subscription(&h.app, &checkout, "sub_grants", &due, "grants").await;
    let paid = payment_event(
        "evt-g-paid",
        "PAYMENT_CONFIRMED",
        "pay_grants",
        "sub_grants",
        &due,
        &format!("{due} 10:00:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, paid).await, StatusCode::OK);
    operator::revoke(&h.pool, "ops@example.test", "ent-grants", &grant)
        .await
        .unwrap();
    let decision = menu(&h.pool, &est, today).await;
    assert_eq!(
        decision.source,
        Some(AccessSource::Contract {
            contract_id: reference
        })
    );
    // Refund the contract while a new grant is active: the grant keeps the capability.
    operator::grant(
        &h.pool,
        request("menu", GrantKind::Courtesy, 10, "ent-grants"),
    )
    .await
    .unwrap();
    let refund = payment_event(
        "evt-g-refund",
        "PAYMENT_REFUNDED",
        "pay_grants",
        "sub_grants",
        &due,
        &format!("{due} 11:00:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, refund).await, StatusCode::OK);
    assert!(matches!(
        menu(&h.pool, &est, today).await.source,
        Some(AccessSource::Grant { .. })
    ));
    assert_eq!(
        menu(&h.pool, &est, today + Duration::days(10)).await.status,
        EntitlementStatus::Expired
    );
    assert_eq!(count_events(&h.pool, &est, "EntitlementGranted").await, 2);
    assert_eq!(count_events(&h.pool, &est, "EntitlementRevoked").await, 1);

    // One trial per offer, bounded length, planned modules and non-admin operators refused.
    operator::grant(
        &h.pool,
        request("menu", GrantKind::Trial, 14, "ent-grants-other"),
    )
    .await
    .unwrap();
    let again = operator::grant(
        &h.pool,
        request("menu", GrantKind::Trial, 14, "ent-grants-other"),
    )
    .await
    .unwrap_err();
    assert_eq!(again.i18n_key(), "error.operator_trial_used");
    let long = operator::grant(
        &h.pool,
        request("menu", GrantKind::Courtesy, 400, "ent-grants-other"),
    )
    .await
    .unwrap_err();
    assert_eq!(long.i18n_key(), "error.operator_period_invalid");
    for (offer, kind) in [
        ("delivery", GrantKind::Beta),
        ("delivery", GrantKind::Courtesy),
        ("bundle_x", GrantKind::Trial),
    ] {
        let error = operator::grant(&h.pool, request(offer, kind, 10, "ent-grants"))
            .await
            .unwrap_err();
        assert!(
            error.i18n_key().starts_with("error.commercial_offer"),
            "{offer}"
        );
    }
    let mut forged = request("menu", GrantKind::Courtesy, 10, "ent-grants");
    forged.operator_email = "ent-grants@example.test";
    assert_eq!(
        operator::grant(&h.pool, forged)
            .await
            .unwrap_err()
            .i18n_key(),
        "error.forbidden"
    );
}

#[tokio::test]
async fn planned_modules_never_grant_operations() {
    let h = harness(true).await;
    let (login, est) = owner(&h.pool, "ent-planned").await;
    let today = billing_now().date_naive();
    // A grant row forged directly in the database still cannot activate a planned module.
    sqlx::query(
        "INSERT INTO entitlement_grants (id, establishment_id, offer_kind, offer_code, grant_kind,
            starts_on, ends_on, reason, granted_by_account_id, created_at)
         VALUES ('forged', ?, 'module', 'delivery', 'beta', ?, ?, 'forged row', ?, datetime('now'))",
    )
    .bind(&est)
    .bind(today)
    .bind(today + Duration::days(30))
    .bind(&login.account.id)
    .execute(&h.pool)
    .await
    .unwrap();
    for capability in Capability::ALL
        .into_iter()
        .filter(|c| *c != Capability::MenuPublish)
    {
        for enforcement in [ENFORCED, Enforcement::NotEnforced] {
            let decision = resolve_capability(&h.pool, enforcement, &est, capability, today)
                .await
                .unwrap();
            assert!(!decision.is_allowed(), "{capability} must stay unavailable");
        }
    }
    // Without billing configured, the available QR Menu stays free as before.
    let free = resolve_capability(
        &h.pool,
        Enforcement::NotEnforced,
        &est,
        Capability::MenuPublish,
        today,
    )
    .await
    .unwrap();
    assert_eq!(free.source, Some(AccessSource::NotEnforced));
}

#[tokio::test]
async fn member_permissions_are_separate_from_commercial_capabilities() {
    let h = harness(true).await;
    admin(&h.pool, "ops2@example.test").await;
    let (alice, paid) = owner(&h.pool, "ent-perm-paid").await;
    let (bob, unpaid) = owner(&h.pool, "ent-perm-unpaid").await;
    let alice_manager = manager(&h.pool, &paid, "manager-paid@example.test").await;
    let bob_manager = manager(&h.pool, &unpaid, "manager-unpaid@example.test").await;
    let today = billing_now().date_naive();
    operator::grant(
        &h.pool,
        GrantRequest {
            operator_email: "ops2@example.test",
            establishment_slug: "ent-perm-paid",
            offer_code: "menu",
            kind: GrantKind::Courtesy,
            starts_on: today,
            ends_on: today + Duration::days(30),
            reason: "permission test",
        },
    )
    .await
    .unwrap();
    let publish =
        |login: &qrcardapio::application::auth::AuthSuccess| format!("_csrf={}", login.csrf_token);

    for login in [&alice, &alice_manager] {
        let res = post_form(&h.app, login, "/dashboard/publish", publish(login)).await;
        assert_eq!(location(&res), "/dashboard", "entitled members can publish");
    }
    for login in [&bob, &bob_manager] {
        let res = post_form(&h.app, login, "/dashboard/publish", publish(login)).await;
        assert_eq!(
            location(&res),
            "/dashboard/billing",
            "no capability, regardless of role"
        );
    }
    // A manager holds the publish permission but never the billing permission.
    for path in [
        "/dashboard/billing/checkout",
        "/dashboard/billing/reconcile",
    ] {
        let res = post_form(&h.app, &alice_manager, path, publish(&alice_manager)).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{path}");
    }
    let denied = authorize(
        &h.pool,
        ENFORCED,
        &paid,
        MemberRole::Manager,
        Operation::ManageBilling,
        today,
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(denied, Denial::PermissionDenied);

    // Suspension blocks new protected operations only and is audited.
    operator::set_operational_status(
        &h.pool,
        "ops2@example.test",
        "ent-perm-paid",
        OperationalStatus::SuspendedSecurity,
        "phishing report",
    )
    .await
    .unwrap();
    let res = post_form(&h.app, &alice, "/dashboard/publish", publish(&alice)).await;
    assert_eq!(location(&res), "/dashboard");
    let denied = authorize(
        &h.pool,
        ENFORCED,
        &paid,
        MemberRole::Owner,
        Operation::PublishMenu,
        today,
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(
        denied,
        Denial::EstablishmentSuspended(OperationalStatus::SuspendedSecurity)
    );
    assert!(
        authorize(
            &h.pool,
            ENFORCED,
            &paid,
            MemberRole::Owner,
            Operation::UnpublishMenu,
            today
        )
        .await
        .unwrap()
        .is_ok()
    );
    let page = get(&h.app, &alice, "/dashboard/billing").await;
    let html = String::from_utf8(
        page.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("commercial.status_suspended"));
    operator::set_operational_status(
        &h.pool,
        "ops2@example.test",
        "ent-perm-paid",
        OperationalStatus::Active,
        "report cleared",
    )
    .await
    .unwrap();
    assert!(
        authorize(
            &h.pool,
            ENFORCED,
            &paid,
            MemberRole::Owner,
            Operation::PublishMenu,
            today
        )
        .await
        .unwrap()
        .is_ok()
    );
    assert_eq!(
        count_events(&h.pool, &paid, "EstablishmentSuspended").await,
        1
    );
    assert_eq!(
        count_events(&h.pool, &paid, "EstablishmentReinstated").await,
        1
    );
    assert_eq!(count_events(&h.pool, &paid, "CatalogPublished").await, 2);
    assert_eq!(count_events(&h.pool, &unpaid, "CatalogPublished").await, 0);
}

#[tokio::test]
async fn subscription_page_lists_modules_without_selling_planned_ones() {
    let h = harness(true).await;
    let (login, est) = owner(&h.pool, "ent-catalog").await;
    let today = billing_now().date_naive();
    let due = today.to_string();
    let (_, checkout) = open_checkout(&h.pool, &est, BillingPlan::Monthly, today).await;
    link_subscription(&h.app, &checkout, "sub_catalog", &due, "catalog").await;
    let paid = payment_event(
        "evt-c-paid",
        "PAYMENT_CONFIRMED",
        "pay_catalog",
        "sub_catalog",
        &due,
        &format!("{due} 10:00:00"),
        json!(29.90),
    );
    assert_eq!(webhook(&h.app, paid).await, StatusCode::OK);

    let page = get(&h.app, &login, "/dashboard/billing").await;
    assert_eq!(page.status(), StatusCode::OK);
    let html = String::from_utf8(
        page.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    for name in [
        "QR Menu",
        "QR Delivery",
        "QR Tables",
        "QR Kitchen",
        "QR Fleet",
        "QR Loyalty",
        "QR Analytics",
        "QR POS",
        "QR Connect",
    ] {
        assert!(html.contains(name), "{name}");
    }
    assert!(html.contains("data-i18n=\"commercial.status_contracted\""));
    assert_eq!(
        html.matches("data-i18n=\"commercial.status_in_development\"")
            .count(),
        8
    );
    for planned_price in ["69,90", "39,90", "24,90", "19,90"] {
        assert!(
            !html.contains(planned_price),
            "planned price {planned_price} must not be announced"
        );
    }
    assert!(!html.contains("value=\"delivery\""));

    // Offers are validated by the backend before any gateway call; prices are never accepted.
    let blocked = format!("_csrf={}&plan=monthly&offer=delivery", login.csrf_token);
    assert_eq!(
        post_form(&h.app, &login, "/dashboard/billing/checkout", blocked)
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let bundle = format!(
        "_csrf={}&plan=annual&offer=bundle:complete",
        login.csrf_token
    );
    assert_eq!(
        post_form(&h.app, &login, "/dashboard/billing/checkout", bundle)
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM billing_checkouts WHERE establishment_id = ?")
            .bind(&est)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
    let totals: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT k.total_centavos, c.price_centavos FROM commercial_contracts k
         JOIN billing_checkouts c ON c.reference = k.billing_reference WHERE k.establishment_id = ?",
    )
    .bind(&est).fetch_all(&h.pool).await.unwrap();
    assert_eq!(totals, [(2990, 2990)]);
}
