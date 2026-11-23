use sqlx::{SqliteConnection, SqlitePool};

use super::mcp_oauth::Grant;
use super::{
    categories::insert_category_in,
    mappers::{row_to_category, row_to_product},
    products::insert_product_in,
};
use crate::domain::{
    category::Category,
    mcp_menu::{AppliedChanges, CategoryPatch, ProductPatch, ValidatedChanges},
    product::Product,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn categories(
    pool: &SqlitePool,
    establishment: &str,
    offset: i64,
    limit: i64,
) -> AppResult<Vec<Category>> {
    let rows = sqlx::query("SELECT * FROM categories WHERE establishment_id = ? ORDER BY position, id LIMIT ? OFFSET ?")
        .bind(establishment).bind(limit).bind(offset).fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_category).collect())
}

pub async fn products(
    pool: &SqlitePool,
    establishment: &str,
    category: &str,
    offset: i64,
    limit: i64,
) -> AppResult<Vec<Product>> {
    let rows = sqlx::query("SELECT * FROM products WHERE category_id = ? AND establishment_id = ? ORDER BY position, id LIMIT ? OFFSET ?")
        .bind(category).bind(establishment).bind(limit).bind(offset).fetch_all(pool).await?;
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
) -> AppResult<AppliedChanges> {
    let mut tx = begin_immediate(pool).await?;
    let est = &grant.establishment_id;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mcp_grants g JOIN establishment_members m
            ON m.establishment_id = g.establishment_id AND m.account_id = g.account_id
         WHERE g.id = ? AND g.account_id = ? AND g.establishment_id = ? AND g.access_expires_at > ?
           AND g.scope = 'menu.read menu.write' AND m.member_role = 'owner')",
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
        return serde_json::from_str(&data)
            .map_err(|_| AppError::Internal("invalid stored MCP result".into()));
    }

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
    };
    let result_json = serde_json::to_string(&result)
        .map_err(|_| AppError::Internal("could not encode MCP result".into()))?;
    sqlx::query(
        "INSERT INTO mcp_menu_operations (establishment_id, account_id, operation_key, input_hash, result_json, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (establishment_id, account_id, operation_key) DO UPDATE SET input_hash = excluded.input_hash,
            result_json = excluded.result_json, expires_at = excluded.expires_at",
    ).bind(est).bind(&grant.account_id).bind(operation.key).bind(operation.hash).bind(result_json)
        .bind(operation.now + 7 * 86400).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result)
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
    ).bind(&text.name).bind(&text.name_en).bind(&text.name_es).bind(&text.description).bind(&text.description_en)
        .bind(&text.description_es).bind(patch.visible).bind(now).bind(&patch.id).bind(est).bind(&patch.updated_at).execute(conn).await?;
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
    let text = &patch.text;
    let result = sqlx::query(
        "UPDATE products SET name = COALESCE(?, name), name_en = COALESCE(?, name_en), name_es = COALESCE(?, name_es),
         description = COALESCE(?, description), description_en = COALESCE(?, description_en), description_es = COALESCE(?, description_es),
         is_visible = COALESCE(?, is_visible), is_available = COALESCE(?, is_available), price_centavos = COALESCE(?, price_centavos),
         updated_at = ? WHERE id = ? AND establishment_id = ? AND updated_at = ?",
    ).bind(&text.name).bind(&text.name_en).bind(&text.name_es).bind(&text.description).bind(&text.description_en)
        .bind(&text.description_es).bind(patch.visible).bind(patch.available).bind(patch.price_centavos)
        .bind(now).bind(&patch.id).bind(est).bind(&patch.updated_at).execute(conn).await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("mcp.menu_changed"));
    }
    Ok(())
}
