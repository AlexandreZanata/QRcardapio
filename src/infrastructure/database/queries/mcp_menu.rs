use std::collections::HashSet;

use sqlx::{Row, SqliteConnection, SqlitePool};

use super::mcp_oauth::Grant;
use super::{
    categories::insert_category_in,
    mappers::{row_to_category, row_to_media, row_to_product},
    products::insert_product_in,
};
use crate::domain::{
    category::Category,
    mcp_menu::{
        AppliedChanges, CategoryPatch, DeleteRef, ProductOrderPatch, ProductPatch, ValidatedChanges,
    },
    product::Product,
    publication::MediaAsset,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub struct CategoryWithCount {
    pub category: Category,
    pub product_count: i64,
}

pub async fn categories_with_counts(
    pool: &SqlitePool,
    establishment: &str,
    offset: i64,
    limit: i64,
) -> AppResult<Vec<CategoryWithCount>> {
    let rows = sqlx::query(
        "SELECT c.*, COUNT(p.id) AS product_count
         FROM categories c
         LEFT JOIN products p ON p.category_id = c.id AND p.establishment_id = c.establishment_id
         WHERE c.establishment_id = ?
         GROUP BY c.id
         ORDER BY c.position, c.id
         LIMIT ? OFFSET ?",
    )
    .bind(establishment)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| CategoryWithCount {
            category: row_to_category(row),
            product_count: row.get("product_count"),
        })
        .collect())
}

pub async fn products(
    pool: &SqlitePool,
    establishment: &str,
    category: &str,
    offset: i64,
    limit: i64,
) -> AppResult<Vec<Product>> {
    let rows = sqlx::query(
        "SELECT * FROM products WHERE category_id = ? AND establishment_id = ? ORDER BY position, id LIMIT ? OFFSET ?",
    )
    .bind(category)
    .bind(establishment)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_product).collect())
}

pub struct Operation<'a> {
    pub key: &'a str,
    pub hash: &'a str,
    pub now: i64,
    pub timestamp: &'a str,
}

pub async fn apply(
    pool: &SqlitePool,
    grant: &Grant,
    operation: Operation<'_>,
    changes: ValidatedChanges,
) -> AppResult<(AppliedChanges, Vec<MediaAsset>)> {
    let mut tx = begin_immediate(pool).await?;
    let est = &grant.establishment_id;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mcp_grants g JOIN establishment_members m
            ON m.establishment_id = g.establishment_id AND m.account_id = g.account_id
         WHERE g.id = ? AND g.account_id = ? AND g.establishment_id = ? AND g.access_expires_at > ?
           AND (' ' || g.scope || ' ') LIKE '% menu.write %' AND m.member_role = 'owner')",
    )
    .bind(&grant.id)
    .bind(&grant.account_id)
    .bind(est)
    .bind(operation.now)
    .fetch_one(&mut *tx)
    .await?;
    if !valid {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    let previous: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT input_hash, result_json, expires_at FROM mcp_menu_operations
         WHERE operation_key = ? AND account_id = ? AND establishment_id = ?",
    )
    .bind(operation.key)
    .bind(&grant.account_id)
    .bind(est)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((hash, data, expires_at)) = previous
        && expires_at > operation.now
    {
        if hash != operation.hash {
            return Err(AppError::Conflict("mcp.operation_conflict"));
        }
        let cached: AppliedChanges = serde_json::from_str(&data)
            .map_err(|_| AppError::Internal("invalid stored MCP result".into()))?;
        return Ok((cached, Vec::new()));
    }

    let mut orphaned_assets = Vec::new();
    for category in &changes.categories {
        insert_category_in(&mut tx, category).await?;
    }
    for product in &changes.products {
        insert_product_in(&mut tx, product).await?;
    }
    for update in &changes.category_updates {
        update_category(&mut tx, est, update, operation.timestamp).await?;
    }
    for update in &changes.product_updates {
        update_product(&mut tx, est, update, operation.timestamp).await?;
    }
    for del in &changes.delete_products {
        if let Some(asset) = delete_product(&mut tx, est, del).await? {
            orphaned_assets.push(asset);
        }
    }
    for del in &changes.delete_categories {
        orphaned_assets.extend(delete_category(&mut tx, est, del).await?);
    }
    let categories_reordered = if let Some(order) = &changes.category_order {
        reorder_categories(&mut tx, est, order, operation.timestamp).await?;
        true
    } else {
        false
    };
    let mut products_reordered = 0usize;
    for order in &changes.product_orders {
        reorder_products(&mut tx, est, order, operation.timestamp).await?;
        products_reordered += order.product_ids.len();
    }

    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(operation.timestamp)
    .bind(est)
    .execute(&mut *tx)
    .await?;

    let result = AppliedChanges {
        category_ids: changes
            .categories
            .into_iter()
            .map(|category| category.id)
            .collect(),
        product_ids: changes
            .products
            .into_iter()
            .map(|product| product.id)
            .collect(),
        categories_updated: changes.category_updates.len(),
        products_updated: changes.product_updates.len(),
        categories_deleted: changes.delete_categories.len(),
        products_deleted: changes.delete_products.len(),
        categories_reordered,
        products_reordered,
    };
    let result_json = serde_json::to_string(&result)
        .map_err(|_| AppError::Internal("could not encode MCP result".into()))?;
    sqlx::query(
        "INSERT INTO mcp_menu_operations (establishment_id, account_id, operation_key, input_hash, result_json, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (establishment_id, account_id, operation_key) DO UPDATE SET input_hash = excluded.input_hash,
            result_json = excluded.result_json, expires_at = excluded.expires_at",
    )
    .bind(est)
    .bind(&grant.account_id)
    .bind(operation.key)
    .bind(operation.hash)
    .bind(result_json)
    .bind(operation.now + 7 * 86400)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((result, orphaned_assets))
}

async fn update_category(
    conn: &mut SqliteConnection,
    est: &str,
    patch: &CategoryPatch,
    now: &str,
) -> AppResult<()> {
    let text = &patch.text;
    let result = sqlx::query(
        "UPDATE categories SET name = COALESCE(?, name), name_en = COALESCE(?, name_en), name_es = COALESCE(?, name_es),
         description = COALESCE(?, description), description_en = COALESCE(?, description_en), description_es = COALESCE(?, description_es),
         is_visible = COALESCE(?, is_visible), updated_at = ? WHERE id = ? AND establishment_id = ? AND updated_at = ?",
    )
    .bind(&text.name)
    .bind(&text.name_en)
    .bind(&text.name_es)
    .bind(&text.description)
    .bind(&text.description_en)
    .bind(&text.description_es)
    .bind(patch.visible)
    .bind(now)
    .bind(&patch.id)
    .bind(est)
    .bind(&patch.updated_at)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("mcp.menu_changed"));
    }
    Ok(())
}

async fn update_product(
    conn: &mut SqliteConnection,
    est: &str,
    patch: &ProductPatch,
    now: &str,
) -> AppResult<()> {
    let mut new_position: Option<i64> = None;
    if let Some(target_cat) = patch.category_id.as_deref() {
        let cat_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM categories WHERE id = ? AND establishment_id = ? LIMIT 1",
        )
        .bind(target_cat)
        .bind(est)
        .fetch_optional(&mut *conn)
        .await?;
        if cat_exists.is_none() {
            return Err(AppError::Forbidden("error.forbidden"));
        }
        let current_cat: Option<String> = sqlx::query_scalar(
            "SELECT category_id FROM products WHERE id = ? AND establishment_id = ? AND updated_at = ?",
        )
        .bind(&patch.id)
        .bind(est)
        .bind(&patch.updated_at)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(current_cat) = current_cat else {
            return Err(AppError::Conflict("mcp.menu_changed"));
        };
        if current_cat != target_cat {
            let next_pos: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(position), 0) + 1 FROM products WHERE establishment_id = ? AND category_id = ?",
            )
            .bind(est)
            .bind(target_cat)
            .fetch_one(&mut *conn)
            .await?;
            new_position = Some(next_pos);
        }
    }

    let text = &patch.text;
    let result = sqlx::query(
        "UPDATE products SET category_id = COALESCE(?, category_id), position = COALESCE(?, position),
         name = COALESCE(?, name), name_en = COALESCE(?, name_en), name_es = COALESCE(?, name_es),
         description = COALESCE(?, description), description_en = COALESCE(?, description_en), description_es = COALESCE(?, description_es),
         is_visible = COALESCE(?, is_visible), is_available = COALESCE(?, is_available), price_centavos = COALESCE(?, price_centavos),
         updated_at = ? WHERE id = ? AND establishment_id = ? AND updated_at = ?",
    )
    .bind(&patch.category_id)
    .bind(new_position)
    .bind(&text.name)
    .bind(&text.name_en)
    .bind(&text.name_es)
    .bind(&text.description)
    .bind(&text.description_en)
    .bind(&text.description_es)
    .bind(patch.visible)
    .bind(patch.available)
    .bind(patch.price_centavos)
    .bind(now)
    .bind(&patch.id)
    .bind(est)
    .bind(&patch.updated_at)
    .execute(conn)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("mcp.menu_changed"));
    }
    Ok(())
}

async fn delete_product(
    conn: &mut SqliteConnection,
    est: &str,
    del: &DeleteRef,
) -> AppResult<Option<MediaAsset>> {
    let row = sqlx::query(
        "SELECT image_asset_id FROM products WHERE id = ? AND establishment_id = ? AND updated_at = ?",
    )
    .bind(&del.id)
    .bind(est)
    .bind(&del.updated_at)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Err(AppError::Conflict("mcp.menu_changed"));
    };
    let image_asset_id: Option<String> = row.get("image_asset_id");
    let asset = if let Some(asset_id) = image_asset_id.as_deref() {
        sqlx::query("SELECT * FROM media_assets WHERE id = ? AND establishment_id = ?")
            .bind(asset_id)
            .bind(est)
            .fetch_optional(&mut *conn)
            .await?
            .map(|r| row_to_media(&r))
    } else {
        None
    };

    let deleted = sqlx::query(
        "DELETE FROM products WHERE id = ? AND establishment_id = ? AND updated_at = ?",
    )
    .bind(&del.id)
    .bind(est)
    .bind(&del.updated_at)
    .execute(&mut *conn)
    .await?;
    if deleted.rows_affected() != 1 {
        return Err(AppError::Conflict("mcp.menu_changed"));
    }
    if let Some(asset_ref) = &asset {
        sqlx::query("DELETE FROM media_assets WHERE id = ? AND establishment_id = ?")
            .bind(&asset_ref.id)
            .bind(est)
            .execute(&mut *conn)
            .await?;
    }
    Ok(asset)
}

async fn delete_category(
    conn: &mut SqliteConnection,
    est: &str,
    del: &DeleteRef,
) -> AppResult<Vec<MediaAsset>> {
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM categories WHERE id = ? AND establishment_id = ? AND updated_at = ? LIMIT 1",
    )
    .bind(&del.id)
    .bind(est)
    .bind(&del.updated_at)
    .fetch_optional(&mut *conn)
    .await?;
    if exists.is_none() {
        return Err(AppError::Conflict("mcp.menu_changed"));
    }

    let media_rows = sqlx::query(
        "SELECT m.* FROM media_assets m
         INNER JOIN products p ON p.image_asset_id = m.id
         WHERE p.category_id = ? AND p.establishment_id = ? AND m.establishment_id = ?",
    )
    .bind(&del.id)
    .bind(est)
    .bind(est)
    .fetch_all(&mut *conn)
    .await?;
    let assets: Vec<MediaAsset> = media_rows.iter().map(row_to_media).collect();

    let deleted = sqlx::query(
        "DELETE FROM categories WHERE id = ? AND establishment_id = ? AND updated_at = ?",
    )
    .bind(&del.id)
    .bind(est)
    .bind(&del.updated_at)
    .execute(&mut *conn)
    .await?;
    if deleted.rows_affected() != 1 {
        return Err(AppError::Conflict("mcp.menu_changed"));
    }
    for asset in &assets {
        sqlx::query("DELETE FROM media_assets WHERE id = ? AND establishment_id = ?")
            .bind(&asset.id)
            .bind(est)
            .execute(&mut *conn)
            .await?;
    }
    Ok(assets)
}

async fn reorder_categories(
    conn: &mut SqliteConnection,
    est: &str,
    ordered_ids: &[String],
    now: &str,
) -> AppResult<()> {
    let existing: Vec<String> =
        sqlx::query_scalar("SELECT id FROM categories WHERE establishment_id = ?")
            .bind(est)
            .fetch_all(&mut *conn)
            .await?;
    let existing_set: HashSet<&str> = existing.iter().map(String::as_str).collect();
    if existing_set.len() != ordered_ids.len()
        || !ordered_ids
            .iter()
            .all(|id| existing_set.contains(id.as_str()))
    {
        return Err(AppError::Validation("mcp.invalid_order"));
    }
    for (idx, id) in ordered_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE categories SET position = ?, updated_at = ? WHERE id = ? AND establishment_id = ?",
        )
        .bind((idx as i64) + 1)
        .bind(now)
        .bind(id)
        .bind(est)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn reorder_products(
    conn: &mut SqliteConnection,
    est: &str,
    patch: &ProductOrderPatch,
    now: &str,
) -> AppResult<()> {
    let cat_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM categories WHERE id = ? AND establishment_id = ? LIMIT 1",
    )
    .bind(&patch.category_id)
    .bind(est)
    .fetch_optional(&mut *conn)
    .await?;
    if cat_exists.is_none() {
        return Err(AppError::Forbidden("error.forbidden"));
    }
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM products WHERE establishment_id = ? AND category_id = ?",
    )
    .bind(est)
    .bind(&patch.category_id)
    .fetch_all(&mut *conn)
    .await?;
    let existing_set: HashSet<&str> = existing.iter().map(String::as_str).collect();
    if existing_set.len() != patch.product_ids.len()
        || !patch
            .product_ids
            .iter()
            .all(|id| existing_set.contains(id.as_str()))
    {
        return Err(AppError::Validation("mcp.invalid_order"));
    }
    for (idx, id) in patch.product_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE products SET position = ?, updated_at = ? WHERE id = ? AND establishment_id = ? AND category_id = ?",
        )
        .bind((idx as i64) + 1)
        .bind(now)
        .bind(id)
        .bind(est)
        .bind(&patch.category_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
