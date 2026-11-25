use sqlx::{Row, SqliteConnection, SqlitePool};

use super::establishments::mark_menu_draft_changed;
use crate::domain::commerce::catalog_options::AddonOption;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn list_for_group(
    pool: &SqlitePool,
    est_id: &str,
    group_id: &str,
) -> AppResult<Vec<AddonOption>> {
    let mut conn = pool.acquire().await?;
    Ok(list_in(&mut conn, est_id, Some(group_id))
        .await?
        .into_iter()
        .map(|(_, option)| option)
        .collect())
}

pub(super) async fn list_in(
    conn: &mut SqliteConnection,
    est_id: &str,
    group_id: Option<&str>,
) -> AppResult<Vec<(String, AddonOption)>> {
    let rows = sqlx::query(
        "SELECT group_id, id, name, name_en, name_es, price_centavos, position, max_quantity, is_available
         FROM addon_options WHERE establishment_id = ? AND (? IS NULL OR group_id = ?)
         ORDER BY group_id, position, id",
    )
    .bind(est_id)
    .bind(group_id)
    .bind(group_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get("group_id"),
                AddonOption {
                    id: row.get("id"),
                    name: row.get("name"),
                    name_en: row.get("name_en"),
                    name_es: row.get("name_es"),
                    price_centavos: row.get("price_centavos"),
                    position: row.get("position"),
                    max_quantity: row.get("max_quantity"),
                    is_available: row.get("is_available"),
                },
            )
        })
        .collect())
}

pub async fn save(
    pool: &SqlitePool,
    est_id: &str,
    group_id: &str,
    option: &AddonOption,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let saved = sqlx::query(
        "INSERT INTO addon_options (id, establishment_id, group_id, name, name_en, name_es,
                                    price_centavos, position, max_quantity, is_available)
         SELECT ?, g.establishment_id, g.id, ?, ?, ?, ?, ?, ?, ? FROM addon_groups g
         WHERE g.id = ? AND g.establishment_id = ?
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, name_en = excluded.name_en,
              name_es = excluded.name_es, price_centavos = excluded.price_centavos,
              position = excluded.position, max_quantity = excluded.max_quantity,
              is_available = excluded.is_available
         WHERE addon_options.group_id = excluded.group_id
           AND addon_options.establishment_id = excluded.establishment_id",
    )
    .bind(&option.id)
    .bind(&option.name)
    .bind(&option.name_en)
    .bind(&option.name_es)
    .bind(option.price_centavos)
    .bind(option.position)
    .bind(option.max_quantity)
    .bind(option.is_available)
    .bind(group_id)
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

pub async fn delete(
    pool: &SqlitePool,
    est_id: &str,
    group_id: &str,
    id: &str,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let deleted = sqlx::query(
        "DELETE FROM addon_options WHERE id = ? AND establishment_id = ? AND group_id = ?",
    )
    .bind(id)
    .bind(est_id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        return Ok(false);
    }
    mark_menu_draft_changed(&mut tx, est_id, now).await?;
    tx.commit().await?;
    Ok(true)
}
