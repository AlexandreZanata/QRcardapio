use sqlx::{Row, SqliteConnection, SqlitePool};

use super::establishments::mark_menu_draft_changed;
use crate::domain::commerce::catalog_options::AddonGroup;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn list_for_establishment(pool: &SqlitePool, est_id: &str) -> AppResult<Vec<AddonGroup>> {
    let mut conn = pool.acquire().await?;
    list_in(&mut conn, est_id).await
}

pub(super) async fn list_in(
    conn: &mut SqliteConnection,
    est_id: &str,
) -> AppResult<Vec<AddonGroup>> {
    let rows = sqlx::query(
        "SELECT id, name, name_en, name_es, min_select, max_select, position
         FROM addon_groups WHERE establishment_id = ? ORDER BY position, id",
    )
    .bind(est_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AddonGroup {
            id: row.get("id"),
            name: row.get("name"),
            name_en: row.get("name_en"),
            name_es: row.get("name_es"),
            min_select: row.get("min_select"),
            max_select: row.get("max_select"),
            position: row.get("position"),
        })
        .collect())
}

pub(super) async fn list_product_links_in(
    conn: &mut SqliteConnection,
    est_id: &str,
) -> AppResult<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT product_id, group_id FROM product_addon_groups WHERE establishment_id = ?
         ORDER BY product_id, position, group_id",
    )
    .bind(est_id)
    .fetch_all(conn)
    .await?)
}

pub async fn save(pool: &SqlitePool, est_id: &str, group: &AddonGroup, now: &str) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let saved = sqlx::query(
        "INSERT INTO addon_groups (id, establishment_id, name, name_en, name_es, min_select, max_select, position)
         SELECT ?, id, ?, ?, ?, ?, ?, ? FROM establishments WHERE id = ?
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, name_en = excluded.name_en,
              name_es = excluded.name_es, min_select = excluded.min_select,
              max_select = excluded.max_select, position = excluded.position
         WHERE addon_groups.establishment_id = excluded.establishment_id",
    )
    .bind(&group.id)
    .bind(&group.name)
    .bind(&group.name_en)
    .bind(&group.name_es)
    .bind(group.min_select)
    .bind(group.max_select)
    .bind(group.position)
    .bind(est_id)
    .execute(&mut *tx)
    .await?;
    if saved.rows_affected() != 1 {
        return Err(AppError::NotFound("error.not_found"));
    }
    mark_menu_draft_changed(&mut tx, est_id, now).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, est_id: &str, id: &str, now: &str) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let deleted = sqlx::query("DELETE FROM addon_groups WHERE id = ? AND establishment_id = ?")
        .bind(id)
        .bind(est_id)
        .execute(&mut *tx)
        .await?;
    if deleted.rows_affected() == 0 {
        return Ok(false);
    }
    mark_menu_draft_changed(&mut tx, est_id, now).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn link_to_product(
    pool: &SqlitePool,
    est_id: &str,
    product_id: &str,
    group_id: &str,
    position: i64,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let linked = sqlx::query(
        "INSERT INTO product_addon_groups (establishment_id, product_id, group_id, position)
         SELECT p.establishment_id, p.id, g.id, ? FROM products p
         JOIN addon_groups g ON g.establishment_id = p.establishment_id
         WHERE p.id = ? AND p.establishment_id = ? AND g.id = ?
         ON CONFLICT(product_id, group_id) DO UPDATE SET position = excluded.position
         WHERE product_addon_groups.establishment_id = excluded.establishment_id",
    )
    .bind(position)
    .bind(product_id)
    .bind(est_id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    if linked.rows_affected() != 1 {
        return Err(AppError::NotFound("error.not_found"));
    }
    mark_menu_draft_changed(&mut tx, est_id, now).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn unlink_from_product(
    pool: &SqlitePool,
    est_id: &str,
    product_id: &str,
    group_id: &str,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let unlinked = sqlx::query(
        "DELETE FROM product_addon_groups WHERE product_id = ? AND establishment_id = ? AND group_id = ?",
    )
    .bind(product_id)
    .bind(est_id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    if unlinked.rows_affected() == 0 {
        return Ok(false);
    }
    mark_menu_draft_changed(&mut tx, est_id, now).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn linked_ids(
    pool: &SqlitePool,
    est_id: &str,
    product_id: &str,
) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT group_id FROM product_addon_groups WHERE product_id = ? AND establishment_id = ?
         ORDER BY position, group_id",
    )
    .bind(product_id)
    .bind(est_id)
    .fetch_all(pool)
    .await?)
}
