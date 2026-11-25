//! Expand migration 20261124000009: legacy "Plano Essencial" subscribers become QR Menu v1
//! contracts without new charges, keeping prices, cycles, Asaas ids, paid periods, affiliate
//! commissions, MCP grants and published menus.

#[path = "commercial/support.rs"]
mod support;

use chrono::{Duration, NaiveDate};
use sqlx::SqlitePool;
use tempfile::TempDir;

use qrcardapio::application::entitlements::{Enforcement, resolve_capability};
use qrcardapio::config::DbSynchronous;
use qrcardapio::domain::entitlements::{AccessSource, Capability, EntitlementStatus};
use qrcardapio::infrastructure::database::queries::billing;
use qrcardapio::infrastructure::database::{MIGRATOR, init_pool, open_pool_at};
use support::*;

const BEFORE_COMMERCIAL: i64 = 20261124000008;

/// id, billing_reference, cycle, months, list, total, eligible, activated_at
type ContractRow = (String, String, String, i64, i64, i64, i64, Option<String>);

async fn exec(pool: &SqlitePool, sql: &'static str) {
    sqlx::raw_sql(sql).execute(pool).await.unwrap();
}

async fn scalar(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Builds a database at the last pre-commercial migration with three legacy subscribers.
async fn legacy_database(dir: &TempDir, due: NaiveDate) {
    let cfg = config(dir, true);
    std::fs::create_dir_all(cfg.db_dir()).unwrap();
    let pool = open_pool_at(&cfg.db_path(), 2, DbSynchronous::Full)
        .await
        .unwrap();
    MIGRATOR.run_to(BEFORE_COMMERCIAL, &pool).await.unwrap();
    let due = due.to_string();
    exec(&pool, "
        INSERT INTO accounts (id, email, password_hash, role, created_at, updated_at) VALUES
          ('acc-a', 'a@legacy.test', 'x', 'owner', '2026-01-01', '2026-01-01'),
          ('acc-b', 'b@legacy.test', 'x', 'owner', '2026-01-01', '2026-01-01'),
          ('acc-c', 'c@legacy.test', 'x', 'owner', '2026-01-01', '2026-01-01'),
          ('acc-p', 'p@legacy.test', 'x', 'owner', '2026-01-01', '2026-01-01');
        INSERT INTO establishments (id, slug, name, is_published, published_version, created_at, updated_at) VALUES
          ('est-a', 'legado-mensal', 'Legado Mensal', 1, 3, '2026-01-01', '2026-01-01'),
          ('est-b', 'legado-semestral', 'Legado Semestral', 1, 1, '2026-01-01', '2026-01-01'),
          ('est-c', 'legado-pendente', 'Legado Pendente', 0, 0, '2026-01-01', '2026-01-01');
        INSERT INTO establishment_members (establishment_id, account_id, member_role, created_at) VALUES
          ('est-a', 'acc-a', 'owner', '2026-01-01'),
          ('est-b', 'acc-b', 'owner', '2026-01-01'),
          ('est-c', 'acc-c', 'owner', '2026-01-01');
        INSERT INTO mcp_grants (id, establishment_id, account_id, client_id, resource, scope, access_hash,
            refresh_hash, access_expires_at, refresh_expires_at, created_at)
          VALUES ('grant-a', 'est-a', 'acc-a', 'claude', 'https://menu.example.test/mcp',
            'menu.read menu.write', 'ah', 'rh', 4102444800, 4102444800, '2026-01-01');
    ").await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("
        INSERT INTO billing_checkouts (reference, environment, establishment_id, checkout_id, checkout_status,
            subscription_id, subscription_status, subscription_event_at, created_at, plan_months, price_centavos) VALUES
          ('ref-a', 'sandbox', 'est-a', '0193a0c0-0000-7000-8000-00000000000a', 'paid', 'sub_a', 'active', '{due} 09:00:00', '2026-01-01 10:00:00', 1, 2990),
          ('ref-b', 'sandbox', 'est-b', '0193a0c0-0000-7000-8000-00000000000b', 'paid', 'sub_b', 'deleted', '{due} 09:00:00', '2026-01-01 10:00:00', 6, 17043),
          ('ref-c', 'sandbox', 'est-c', '0193a0c0-0000-7000-8000-00000000000c', 'open', NULL, 'pending', NULL, '2026-01-01 10:00:00', 1, 2990);
        INSERT INTO billing_payments (environment, payment_id, subscription_id, due_date, event_at, event_priority, confirmed_centavos) VALUES
          ('sandbox', 'pay_a', 'sub_a', '{due}', '{due} 10:00:00', 1, 2990),
          ('sandbox', 'pay_b', 'sub_b', '{due}', '{due} 10:00:00', 1, 17043);
        INSERT INTO billing_webhook_events (environment, event_id) VALUES ('sandbox', 'evt-legacy');
        INSERT INTO affiliates (id, account_id, code, status, commission_type, commission_value, hold_days, created_at, updated_at)
          VALUES ('aff-1', 'acc-p', 'parceiro', 'active', 'fixed_monthly_centavos', 500, 15, '2026-01-01', '2026-01-01');
        INSERT INTO affiliate_referrals (id, affiliate_id, referred_account_id, referred_establishment_id, status, attributed_at, first_paid_at, updated_at)
          VALUES ('ref-1', 'aff-1', 'acc-a', 'est-a', 'active', '2026-01-01', '2026-01-01', '2026-01-01');
        INSERT INTO affiliate_commissions (id, environment, affiliate_id, referral_id, establishment_id, payment_id,
            subscription_id, entry_kind, installment_index, billing_cycle_months, gross_payment_centavos,
            commission_centavos, snapshot_commission_type, snapshot_commission_value, status, available_on,
            created_at, updated_at)
          VALUES ('com-1', 'sandbox', 'aff-1', 'ref-1', 'est-a', 'pay_a', 'sub_a', 'commission', 1, 1, 2990,
            500, 'fixed_monthly_centavos', 500, 'pending', '{due}', '2026-01-01', '2026-01-01');
        INSERT INTO affiliate_evaluated_payments (environment, payment_id, evaluated_at) VALUES ('sandbox', 'pay_a', '2026-01-01');
    "))).execute(&pool).await.unwrap();
    pool.close().await;
}

#[tokio::test]
async fn legacy_subscribers_become_menu_contracts_without_new_charges() {
    let dir = TempDir::new().unwrap();
    let today = qrcardapio::domain::billing::billing_now().date_naive();
    legacy_database(&dir, today).await;
    let cfg = config(&dir, true);
    let pool = init_pool(&cfg).await.unwrap();
    let before_coverage = billing::paid_coverage(&pool, ENV, "est-b", today)
        .await
        .unwrap()
        .unwrap();

    // No gateway-facing rows were created or changed.
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM billing_checkouts").await,
        3
    );
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM billing_payments").await,
        2
    );
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM billing_webhook_events").await,
        1
    );
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM domain_events").await,
        0,
        "migration emits no events"
    );

    let contracts: Vec<ContractRow> = sqlx::query_as(
        "SELECT id, billing_reference, cycle_code, cycle_months, list_total_centavos, total_centavos,
                affiliate_eligible_centavos, activated_at
         FROM commercial_contracts WHERE origin = 'legacy_plan' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(contracts.len(), 3);
    assert_eq!(&contracts[0].0, "ref-a");
    assert_eq!(
        (
            contracts[0].2.as_str(),
            contracts[0].3,
            contracts[0].4,
            contracts[0].5
        ),
        ("monthly", 1, 2990, 2990)
    );
    assert_eq!(
        (
            contracts[1].2.as_str(),
            contracts[1].3,
            contracts[1].4,
            contracts[1].5,
            contracts[1].6
        ),
        ("semiannual", 6, 17940, 17043, 17043)
    );
    assert!(contracts[0].7.is_some() && contracts[1].7.is_some());
    assert!(contracts[2].7.is_none(), "unpaid checkout is not activated");
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM commercial_contract_items WHERE offer_code = 'menu' AND offer_version = 1").await, 3);
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM commercial_contract_capabilities WHERE capability = 'menu.publish'").await, 3);

    // Paid periods are identical to the legacy coverage; cancelled renewal keeps its period.
    let enforced = Enforcement::Enforced { environment: ENV };
    for (est, contract) in [("est-a", "ref-a"), ("est-b", "ref-b")] {
        let decision = resolve_capability(&pool, enforced, est, Capability::MenuPublish, today)
            .await
            .unwrap();
        assert_eq!(decision.status, EntitlementStatus::Active, "{est}");
        assert_eq!(
            decision.source,
            Some(AccessSource::Contract {
                contract_id: contract.into()
            })
        );
        let legacy = billing::paid_through(&pool, ENV, est, today).await.unwrap();
        assert_eq!(decision.ends_on, legacy, "{est}");
    }
    assert_eq!(
        before_coverage.paid_through,
        today.checked_add_months(chrono::Months::new(6)).unwrap()
    );
    let pending = resolve_capability(&pool, enforced, "est-c", Capability::MenuPublish, today)
        .await
        .unwrap();
    assert_eq!(pending.status, EntitlementStatus::NotContracted);
    let after = resolve_capability(
        &pool,
        enforced,
        "est-a",
        Capability::MenuPublish,
        today + Duration::days(40),
    )
    .await
    .unwrap();
    assert_eq!(after.status, EntitlementStatus::Expired);

    // Affiliate ledger, MCP grants and publication state are untouched.
    assert_eq!(
        scalar(
            &pool,
            "SELECT SUM(commission_centavos) FROM affiliate_commissions"
        )
        .await,
        500
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM mcp_grants WHERE scope = 'menu.read menu.write'"
        )
        .await,
        1
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT SUM(published_version) FROM establishments WHERE is_published = 1"
        )
        .await,
        4
    );
    let slug: String = sqlx::query_scalar("SELECT slug FROM establishments WHERE id = 'est-a'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(slug, "legado-mensal");

    // Re-applying the idempotent expand migration (manual rollback procedure) changes nothing.
    let sql = MIGRATOR
        .iter()
        .find(|m| m.version == 20261124000009)
        .unwrap()
        .sql
        .clone();
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM commercial_contracts").await,
        3
    );
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM commercial_contract_items").await,
        3
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM commercial_contract_capabilities"
        )
        .await,
        3
    );
    pool.close().await;

    // A binary restarted on the migrated database (rollback-tolerant migrator) boots again.
    let pool = init_pool(&cfg).await.unwrap();
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM commercial_contracts").await,
        3
    );
}

#[tokio::test]
async fn migrated_subscriber_renewal_activates_and_keeps_contracted_price() {
    let dir = TempDir::new().unwrap();
    let today = qrcardapio::domain::billing::billing_now().date_naive();
    legacy_database(&dir, today).await;
    let cfg = config(&dir, true);
    let pool = init_pool(&cfg).await.unwrap();
    let app = qrcardapio::app::build_router(qrcardapio::app::AppState::new(cfg, pool.clone()));
    // The pending legacy checkout is paid after the upgrade: it activates with its legacy price.
    let due = today.to_string();
    link_subscription(
        &app,
        "0193a0c0-0000-7000-8000-00000000000c",
        "sub_c",
        &due,
        "legacy-c",
    )
    .await;
    let paid = payment_event(
        "evt-c",
        "PAYMENT_CONFIRMED",
        "pay_c",
        "sub_c",
        &due,
        &format!("{due} 11:00:00"),
        serde_json::json!(29.90),
    );
    assert_eq!(webhook(&app, paid).await, axum::http::StatusCode::OK);
    let decision = resolve_capability(
        &pool,
        Enforcement::Enforced { environment: ENV },
        "est-c",
        Capability::MenuPublish,
        today,
    )
    .await
    .unwrap();
    assert!(decision.is_allowed());
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM domain_events WHERE event_type = 'SubscriptionActivated'"
        )
        .await,
        1
    );
}
