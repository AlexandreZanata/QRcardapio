use std::time::Instant;

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::publishing::{PublishLocks, publish_establishment};
use crate::config::Config;
use crate::error::AppResult;

pub struct BenchGenStats {
    pub establishments_created: usize,
    pub categories_created: usize,
    pub products_created: usize,
    pub menus_published: usize,
    pub first_slug: Option<String>,
    pub elapsed_ms: u128,
    pub db_bytes: u64,
}

/// Generates synthetic multi-tenant benchmark data in batched transactions
/// to evaluate storage, publish latency, and server behavior at 100..100,000 establishments.
pub async fn generate_benchmark_dataset(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    count: usize,
    items_per_est: usize,
    publish_sample: usize,
) -> AppResult<BenchGenStats> {
    let started = Instant::now();
    let now = Utc::now().to_rfc3339();
    // Pre-computed dummy PHC hash so generating 100k synthetic tenants remains fast and deterministic.
    let dummy_pw_hash =
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0c2FsdA$12345678901234567890123456789012";

    let batch_size = 500usize;
    let mut created = 0usize;
    let mut cats_created = 0usize;
    let mut prods_created = 0usize;
    let mut published_ids: Vec<String> = Vec::new();
    let mut first_slug: Option<String> = None;
    let run_tag = &Uuid::now_v7().to_string()[..8];

    while created < count {
        let end = (created + batch_size).min(count);
        let mut tx = pool.begin().await?;

        for i in created..end {
            let acc_id = Uuid::now_v7().to_string();
            let est_id = Uuid::now_v7().to_string();
            let slug = format!("bench-{run_tag}-{:06}", i + 1);
            if first_slug.is_none() {
                first_slug = Some(slug.clone());
            }
            let email = format!("bench-{run_tag}-{:06}@example.invalid", i + 1);
            let est_name = format!("Estabelecimento Teste #{}", i + 1);

            sqlx::query(
                "INSERT INTO accounts (id, email, password_hash, role, preferred_locale, created_at, updated_at)
                 VALUES (?, ?, ?, 'owner', 'pt-BR', ?, ?)",
            )
            .bind(&acc_id)
            .bind(&email)
            .bind(dummy_pw_hash)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO establishments (id, slug, name, description, accent_color, is_published, published_version, has_unpublished_changes, created_at, updated_at)
                 VALUES (?, ?, ?, 'Cardápio demonstrativo para benchmark.', '#D9532F', 0, 0, 1, ?, ?)",
            )
            .bind(&est_id)
            .bind(&slug)
            .bind(&est_name)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO establishment_members (establishment_id, account_id, member_role, created_at)
                 VALUES (?, ?, 'owner', ?)",
            )
            .bind(&est_id)
            .bind(&acc_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            let mut cat_ids = Vec::with_capacity(3);
            for c_idx in 0..3 {
                let cid = Uuid::now_v7().to_string();
                let cname = match c_idx {
                    0 => "Entradas",
                    1 => "Pratos Principais",
                    _ => "Bebidas & Sobremesas",
                };
                sqlx::query(
                    "INSERT INTO categories (id, establishment_id, name, position, is_visible, created_at, updated_at)
                     VALUES (?, ?, ?, ?, 1, ?, ?)",
                )
                .bind(&cid)
                .bind(&est_id)
                .bind(cname)
                .bind((c_idx + 1) as i64)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                cat_ids.push(cid);
                cats_created += 1;
            }

            for p_idx in 0..items_per_est {
                let pid = Uuid::now_v7().to_string();
                let cid = &cat_ids[p_idx % cat_ids.len()];
                let pname = format!("Item Artesanal #{}", p_idx + 1);
                let price_centavos = 1290 + ((p_idx as i64) * 350);
                sqlx::query(
                    "INSERT INTO products (id, establishment_id, category_id, name, description, price_centavos, position, is_available, is_visible, created_at, updated_at)
                     VALUES (?, ?, ?, ?, 'Preparado na hora com ingredientes selecionados.', ?, ?, 1, 1, ?, ?)",
                )
                .bind(&pid)
                .bind(&est_id)
                .bind(cid)
                .bind(&pname)
                .bind(price_centavos)
                .bind((p_idx + 1) as i64)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                prods_created += 1;
            }

            if published_ids.len() < publish_sample {
                published_ids.push(est_id);
            }
        }

        tx.commit().await?;
        created = end;
    }

    let _ = sqlx::query("PRAGMA wal_checkpoint(PASSIVE);")
        .execute(pool)
        .await;

    let mut menus_published = 0usize;
    for est_id in published_ids {
        publish_establishment(pool, config, locks, &est_id).await?;
        menus_published += 1;
    }

    let db_bytes = std::fs::metadata(config.db_path())
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(BenchGenStats {
        establishments_created: created,
        categories_created: cats_created,
        products_created: prods_created,
        menus_published,
        first_slug,
        elapsed_ms: started.elapsed().as_millis(),
        db_bytes,
    })
}
