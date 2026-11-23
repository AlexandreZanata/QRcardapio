use sqlx::{Row, SqlitePool};

use super::mappers::row_to_product;
use crate::domain::product::Product;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn list_products_for_est(pool: &SqlitePool, est_id: &str) -> AppResult<Vec<Product>> {
    let rows = sqlx::query(
        "SELECT * FROM products
         WHERE establishment_id = ?
         ORDER BY category_id ASC, position ASC, id ASC",
    )
    .bind(est_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_product).collect())
}

pub async fn find_product_for_est(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
) -> AppResult<Option<Product>> {
    let row = sqlx::query("SELECT * FROM products WHERE id = ? AND establishment_id = ?")
        .bind(prod_id)
        .bind(est_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_product(&r)))
}

pub async fn insert_product(pool: &SqlitePool, prod: &Product) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let cat_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM categories WHERE id = ? AND establishment_id = ? LIMIT 1",
    )
    .bind(&prod.category_id)
    .bind(&prod.establishment_id)
    .fetch_optional(&mut *tx)
    .await?;

    if cat_exists.is_none() {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    let next_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), 0) + 1
         FROM products WHERE establishment_id = ? AND category_id = ?",
    )
    .bind(&prod.establishment_id)
    .bind(&prod.category_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO products (
            id, establishment_id, category_id, name, name_en, name_es,
            description, description_en, description_es, price_centavos,
            image_asset_id, position, is_available, is_visible, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&prod.id)
    .bind(&prod.establishment_id)
    .bind(&prod.category_id)
    .bind(&prod.name)
    .bind(&prod.name_en)
    .bind(&prod.name_es)
    .bind(&prod.description)
    .bind(&prod.description_en)
    .bind(&prod.description_es)
    .bind(prod.price_centavos)
    .bind(&prod.image_asset_id)
    .bind(next_pos)
    .bind(i64::from(prod.is_available))
    .bind(i64::from(prod.is_visible))
    .bind(&prod.created_at)
    .bind(&prod.updated_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(&prod.updated_at)
    .bind(&prod.establishment_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_product_for_est(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
    category_id: &str,
    name: &str,
    name_en: &str,
    name_es: &str,
    description: &str,
    description_en: &str,
    description_es: &str,
    price_centavos: i64,
    image_asset_id: Option<&str>,
    is_available: bool,
    is_visible: bool,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;

    let cat_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM categories WHERE id = ? AND establishment_id = ? LIMIT 1",
    )
    .bind(category_id)
    .bind(est_id)
    .fetch_optional(&mut *tx)
    .await?;

    if cat_exists.is_none() {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    let res = sqlx::query(
        "UPDATE products
         SET category_id = ?, name = ?, name_en = ?, name_es = ?,
             description = ?, description_en = ?, description_es = ?,
             price_centavos = ?, image_asset_id = ?,
             is_available = ?, is_visible = ?, updated_at = ?
         WHERE id = ? AND establishment_id = ?",
    )
    .bind(category_id)
    .bind(name)
    .bind(name_en)
    .bind(name_es)
    .bind(description)
    .bind(description_en)
    .bind(description_es)
    .bind(price_centavos)
    .bind(image_asset_id)
    .bind(i64::from(is_available))
    .bind(i64::from(is_visible))
    .bind(now)
    .bind(prod_id)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

pub async fn toggle_product_availability_for_est(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
    now: &str,
) -> AppResult<Option<bool>> {
    let mut tx = begin_immediate(pool).await?;
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT is_available FROM products WHERE id = ? AND establishment_id = ?",
    )
    .bind(prod_id)
    .bind(est_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(curr) = current else {
        return Ok(None);
    };
    let next = i64::from(curr == 0);

    sqlx::query(
        "UPDATE products SET is_available = ?, updated_at = ? WHERE id = ? AND establishment_id = ?",
    )
    .bind(next)
    .bind(now)
    .bind(prod_id)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(next == 1))
}

pub async fn delete_product_for_est(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let res = sqlx::query("DELETE FROM products WHERE id = ? AND establishment_id = ?")
        .bind(prod_id)
        .bind(est_id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

pub async fn move_product_for_est(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
    move_up: bool,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let cat_id: Option<String> = sqlx::query_scalar(
        "SELECT category_id FROM products WHERE id = ? AND establishment_id = ?",
    )
    .bind(prod_id)
    .bind(est_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(cat_id) = cat_id else {
        return Ok(false);
    };

    let rows = sqlx::query(
        "SELECT id, position FROM products
         WHERE establishment_id = ? AND category_id = ?
         ORDER BY position ASC, id ASC",
    )
    .bind(est_id)
    .bind(&cat_id)
    .fetch_all(&mut *tx)
    .await?;

    let items: Vec<(String, i64)> = rows
        .iter()
        .map(|r| (r.get::<String, _>("id"), r.get::<i64, _>("position")))
        .collect();

    let Some(idx) = items.iter().position(|(id, _)| id == prod_id) else {
        return Ok(false);
    };
    let swap_idx = if move_up {
        if idx == 0 {
            return Ok(true);
        }
        idx - 1
    } else {
        if idx + 1 >= items.len() {
            return Ok(true);
        }
        idx + 1
    };

    let (id_a, pos_a) = (&items[idx].0, items[idx].1);
    let (id_b, pos_b) = (&items[swap_idx].0, items[swap_idx].1);
    let (new_pos_a, new_pos_b) = if pos_a == pos_b {
        ((swap_idx as i64) + 1, (idx as i64) + 1)
    } else {
        (pos_b, pos_a)
    };

    sqlx::query(
        "UPDATE products SET position = ?, updated_at = ? WHERE id = ? AND establishment_id = ?",
    )
    .bind(new_pos_a)
    .bind(now)
    .bind(id_a)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE products SET position = ?, updated_at = ? WHERE id = ? AND establishment_id = ?",
    )
    .bind(new_pos_b)
    .bind(now)
    .bind(id_b)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}
