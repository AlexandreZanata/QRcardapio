use std::collections::HashMap;

use sqlx::SqlitePool;
use uuid::Uuid;

use super::mappers::{row_to_category, row_to_establishment, row_to_media, row_to_product};
use crate::domain::publication::{MediaAsset, MenuSnapshot, SnapshotCategory, SnapshotProduct};
use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

pub async fn load_menu_snapshot(
    pool: &SqlitePool,
    est_id: &str,
    include_hidden: bool,
) -> AppResult<Option<MenuSnapshot>> {
    let mut conn = pool.acquire().await?;

    let est_row = sqlx::query("SELECT * FROM establishments WHERE id = ?")
        .bind(est_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(est_row) = est_row else {
        return Ok(None);
    };
    let establishment = row_to_establishment(&est_row);

    let media_rows = sqlx::query("SELECT * FROM media_assets WHERE establishment_id = ?")
        .bind(est_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut media_map: HashMap<String, MediaAsset> = media_rows
        .iter()
        .map(|r| {
            let m = row_to_media(r);
            (m.id.clone(), m)
        })
        .collect();

    let logo = establishment
        .logo_asset_id
        .as_ref()
        .and_then(|id| media_map.get(id).cloned());

    let cat_rows = if include_hidden {
        sqlx::query(
            "SELECT * FROM categories WHERE establishment_id = ? ORDER BY position ASC, id ASC",
        )
        .bind(est_id)
        .fetch_all(&mut *conn)
        .await?
    } else {
        sqlx::query(
            "SELECT * FROM categories WHERE establishment_id = ? AND is_visible = 1 ORDER BY position ASC, id ASC",
        )
        .bind(est_id)
        .fetch_all(&mut *conn)
        .await?
    };

    let prod_rows = if include_hidden {
        sqlx::query(
            "SELECT * FROM products WHERE establishment_id = ? ORDER BY position ASC, id ASC",
        )
        .bind(est_id)
        .fetch_all(&mut *conn)
        .await?
    } else {
        sqlx::query(
            "SELECT * FROM products WHERE establishment_id = ? AND is_visible = 1 ORDER BY position ASC, id ASC",
        )
        .bind(est_id)
        .fetch_all(&mut *conn)
        .await?
    };

    let mut products_by_cat: HashMap<String, Vec<SnapshotProduct>> = HashMap::new();
    for r in &prod_rows {
        let product = row_to_product(r);
        let image = product
            .image_asset_id
            .as_ref()
            .and_then(|id| media_map.get(id).cloned());
        products_by_cat
            .entry(product.category_id.clone())
            .or_default()
            .push(SnapshotProduct { product, image });
    }
    media_map.clear();

    let categories = cat_rows
        .iter()
        .map(|r| {
            let category = row_to_category(r);
            let products = products_by_cat.remove(&category.id).unwrap_or_default();
            SnapshotCategory { category, products }
        })
        .collect();

    Ok(Some(MenuSnapshot {
        establishment,
        logo,
        categories,
    }))
}

pub async fn record_publication_success(
    pool: &SqlitePool,
    est_id: &str,
    new_version: i64,
    html_bytes: i64,
    asset_count: i64,
    duration_ms: i64,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    sqlx::query(
        "UPDATE establishments
         SET is_published = 1, published_version = ?, has_unpublished_changes = 0,
             last_published_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(new_version)
    .bind(now)
    .bind(now)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO publications (id, establishment_id, version, status, html_bytes, asset_count, duration_ms, error_message, published_at)
         VALUES (?, ?, ?, 'published', ?, ?, ?, NULL, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(est_id)
    .bind(new_version)
    .bind(html_bytes)
    .bind(asset_count)
    .bind(duration_ms)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn record_publication_failure(
    pool: &SqlitePool,
    est_id: &str,
    attempted_version: i64,
    error_message: &str,
    duration_ms: i64,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO publications (id, establishment_id, version, status, html_bytes, asset_count, duration_ms, error_message, published_at)
         VALUES (?, ?, ?, 'failed', 0, 0, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(est_id)
    .bind(attempted_version)
    .bind(duration_ms)
    .bind(error_message)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_unpublish(
    pool: &SqlitePool,
    est_id: &str,
    current_version: i64,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    sqlx::query(
        "UPDATE establishments
         SET is_published = 0, has_unpublished_changes = 1, updated_at = ?
         WHERE id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO publications (id, establishment_id, version, status, html_bytes, asset_count, duration_ms, error_message, published_at)
         VALUES (?, ?, ?, 'unpublished', 0, 0, 0, NULL, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(est_id)
    .bind(current_version)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn list_published_establishment_ids(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let ids = sqlx::query_scalar("SELECT id FROM establishments WHERE is_published = 1")
        .fetch_all(pool)
        .await?;
    Ok(ids)
}
