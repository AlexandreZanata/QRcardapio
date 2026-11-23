use sqlx::{Row, SqliteConnection, SqlitePool};

use super::mappers::row_to_category;
use crate::domain::category::Category;
use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

pub async fn list_categories_for_est(pool: &SqlitePool, est_id: &str) -> AppResult<Vec<Category>> {
    let rows = sqlx::query(
        "SELECT * FROM categories
         WHERE establishment_id = ?
         ORDER BY position ASC, id ASC",
    )
    .bind(est_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_category).collect())
}

pub async fn find_category_for_est(
    pool: &SqlitePool,
    est_id: &str,
    cat_id: &str,
) -> AppResult<Option<Category>> {
    let row = sqlx::query("SELECT * FROM categories WHERE id = ? AND establishment_id = ?")
        .bind(cat_id)
        .bind(est_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_category(&r)))
}

pub async fn insert_category(pool: &SqlitePool, cat: &Category) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    insert_category_in(&mut tx, cat).await?;
    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(&cat.updated_at)
    .bind(&cat.establishment_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub(super) async fn insert_category_in(
    conn: &mut SqliteConnection,
    cat: &Category,
) -> AppResult<()> {
    let next_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM categories WHERE establishment_id = ?",
    )
    .bind(&cat.establishment_id)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::query(
        "INSERT INTO categories (
            id, establishment_id, name, name_en, name_es,
            description, description_en, description_es,
            position, is_visible, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&cat.id)
    .bind(&cat.establishment_id)
    .bind(&cat.name)
    .bind(&cat.name_en)
    .bind(&cat.name_es)
    .bind(&cat.description)
    .bind(&cat.description_en)
    .bind(&cat.description_es)
    .bind(next_pos)
    .bind(i64::from(cat.is_visible))
    .bind(&cat.created_at)
    .bind(&cat.updated_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_category_for_est(
    pool: &SqlitePool,
    est_id: &str,
    cat_id: &str,
    name: &str,
    name_en: &str,
    name_es: &str,
    description: &str,
    description_en: &str,
    description_es: &str,
    is_visible: bool,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let res = sqlx::query(
        "UPDATE categories
         SET name = ?, name_en = ?, name_es = ?, description = ?, description_en = ?,
             description_es = ?, is_visible = ?, updated_at = ?
         WHERE id = ? AND establishment_id = ?",
    )
    .bind(name)
    .bind(name_en)
    .bind(name_es)
    .bind(description)
    .bind(description_en)
    .bind(description_es)
    .bind(i64::from(is_visible))
    .bind(now)
    .bind(cat_id)
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

pub async fn delete_category_for_est(
    pool: &SqlitePool,
    est_id: &str,
    cat_id: &str,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let res = sqlx::query("DELETE FROM categories WHERE id = ? AND establishment_id = ?")
        .bind(cat_id)
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

pub async fn move_category_for_est(
    pool: &SqlitePool,
    est_id: &str,
    cat_id: &str,
    move_up: bool,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let rows = sqlx::query(
        "SELECT id, position FROM categories
         WHERE establishment_id = ?
         ORDER BY position ASC, id ASC",
    )
    .bind(est_id)
    .fetch_all(&mut *tx)
    .await?;

    let items: Vec<(String, i64)> = rows
        .iter()
        .map(|r| (r.get::<String, _>("id"), r.get::<i64, _>("position")))
        .collect();

    let Some(idx) = items.iter().position(|(id, _)| id == cat_id) else {
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
        "UPDATE categories SET position = ?, updated_at = ? WHERE id = ? AND establishment_id = ?",
    )
    .bind(new_pos_a)
    .bind(now)
    .bind(id_a)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE categories SET position = ?, updated_at = ? WHERE id = ? AND establishment_id = ?",
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
