use axum::http::StatusCode;
use chrono::Duration;
use sqlx::SqlitePool;

use qrcardapio::application::auth::{AuthSuccess, issue_session};
use qrcardapio::application::demo_menu::seed_public_demo;
use qrcardapio::application::operator::{GrantRequest, grant};
use qrcardapio::domain::account::{Account, AccountRole};
use qrcardapio::domain::billing::billing_now;
use qrcardapio::domain::entitlements::GrantKind;
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::database::queries::{
    catalog_snapshots, delivery_zones, insert_account,
};

use crate::support::{Harness, admin, config, post_form};

pub const ORIGIN: &str = "https://menu.example.test";
pub const MENU_PATH: &str = "/c/bistro-da-praca";
pub const SUBTOTAL: i64 = 4_500;
pub const DELIVERY_FEE: i64 = 500;
const OPERATOR: &str = "delivery-capacity-operator@example.test";
const FIXTURE_TIME: &str = "2026-11-27T00:00:00+00:00";

pub struct Tenant {
    pub id: String,
    pub login: AuthSuccess,
    pub index_url: String,
    pub quote_url: String,
    pub order_url: String,
    pub product: String,
    pub variation: String,
    pub addon: String,
    pub zone: String,
    pub catalog_version: i64,
}

pub async fn seed(harness: &Harness, establishments: usize) -> Vec<Tenant> {
    let pool = &harness.pool;
    admin(pool, OPERATOR).await;
    seed_public_demo(pool, &config(&harness.dir, false))
        .await
        .unwrap();
    let mut tenants = Vec::with_capacity(establishments);
    for index in 0..establishments {
        let id = format!("bench-delivery-{index:03}");
        let login = seed_draft(pool, &id).await;
        let today = billing_now().date_naive();
        grant(
            pool,
            GrantRequest {
                operator_email: OPERATOR,
                establishment_slug: &id,
                offer_code: "delivery",
                kind: GrantKind::InternalPreview,
                starts_on: today - Duration::days(1),
                ends_on: today + Duration::days(1),
                reason: "Synthetic Delivery capacity fixture",
            },
        )
        .await
        .unwrap();
        let submit = |path, details| {
            post_form(
                &harness.app,
                &login,
                path,
                format!("_csrf={}&{details}", login.csrf_token),
            )
        };
        let zone = submit(
            "/dashboard/delivery/zones",
            "neighborhood=Centro&city=Sao+Paulo&fee=5%2C00&min_order=30%2C00&eta_min_minutes=20&eta_max_minutes=40&is_active=1",
        )
        .await;
        assert_eq!(zone.status(), StatusCode::SEE_OTHER, "fixture zone {id}");
        let zone = delivery_zones::list_active_for_establishment(pool, &id)
            .await
            .unwrap()
            .pop()
            .expect("fixture zone");
        assert_eq!(zone.fee_centavos, DELIVERY_FEE);

        let publication = submit("/dashboard/delivery/publish", "").await;
        assert_eq!(publication.status(), StatusCode::SEE_OTHER, "fixture {id}");
        let current_catalog = catalog_snapshots::current(pool, &id, "storefront")
            .await
            .unwrap()
            .expect("committed storefront catalog");
        tenants.push(Tenant {
            index_url: format!("/s/{id}/"),
            quote_url: format!("/api/storefront/v1/{id}/quotes"),
            order_url: format!("/api/storefront/v1/{id}/orders"),
            product: format!("{id}-product"),
            variation: format!("{id}-variation"),
            addon: format!("{id}-addon"),
            catalog_version: current_catalog.version,
            zone: zone.id,
            id,
            login,
        });
    }
    tenants
}

async fn seed_draft(pool: &SqlitePool, id: &str) -> AuthSuccess {
    let account = Account {
        id: format!("{id}-owner"),
        email: format!("{id}-owner@example.test"),
        password_hash: String::new(),
        role: AccountRole::Owner,
        preferred_locale: Locale::PtBr,
        created_at: FIXTURE_TIME.into(),
        updated_at: FIXTURE_TIME.into(),
    };
    insert_account(pool, &account).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO establishments (id, slug, name, created_at, updated_at)
         VALUES (?1, ?1, 'Loja sintética de Delivery', ?2, ?2)",
    )
    .bind(id)
    .bind(FIXTURE_TIME)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO establishment_members (establishment_id, account_id, created_at)
         VALUES (?, ?, ?)",
    )
    .bind(id)
    .bind(&account.id)
    .bind(FIXTURE_TIME)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commercial_settings (establishment_id, delivery, pickup, acceptance_minutes) VALUES (?, 1, 1, 30)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO business_hours (establishment_id, weekday, windows_json)
         SELECT ?, value, '[[0,1440]]' FROM json_each('[0,1,2,3,4,5,6]')",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    for sql in [
        "INSERT INTO categories (id, establishment_id, name, position, created_at, updated_at)
         VALUES (?1 || '-cat', ?1, 'Lanches', 0, ?2, ?2)",
        "INSERT INTO products (id, establishment_id, category_id, name, price_centavos, position, created_at, updated_at)
         VALUES (?1 || '-product', ?1, ?1 || '-cat', 'Sanduíche', 1500, 0, ?2, ?2)",
    ] {
        sqlx::query(sql)
            .bind(id)
            .bind(FIXTURE_TIME)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    for sql in [
        "INSERT INTO product_variations (id, establishment_id, product_id, name, price_centavos, position)
         VALUES (?1 || '-variation', ?1, ?1 || '-product', 'Grande', 2000, 0)",
        "INSERT INTO addon_groups (id, establishment_id, name, min_select, max_select, position)
         VALUES (?1 || '-group', ?1, 'Extras', 1, 1, 0)",
        "INSERT INTO addon_options (id, establishment_id, group_id, name, price_centavos, max_quantity, position)
         VALUES (?1 || '-addon', ?1, ?1 || '-group', 'Queijo', 125, 2, 0)",
        "INSERT INTO product_addon_groups (establishment_id, product_id, group_id, position)
         VALUES (?1, ?1 || '-product', ?1 || '-group', 0)",
    ] {
        sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    issue_session(pool, account).await.unwrap()
}
