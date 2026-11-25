use sqlx::{Row, SqliteConnection, SqlitePool};

use super::establishments::mark_menu_draft_changed;
use crate::domain::commerce::catalog_options::ProductVariation;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn list_for_product(
    pool: &SqlitePool,
    est_id: &str,
    product_id: &str,
) -> AppResult<Vec<ProductVariation>> {
    let mut conn = pool.acquire().await?;
    Ok(list_in(&mut conn, est_id, Some(product_id))
        .await?
        .into_iter()
        .map(|(_, variation)| variation)
        .collect())
}

pub(super) async fn list_in(
    conn: &mut SqliteConnection,
    est_id: &str,
    product_id: Option<&str>,
) -> AppResult<Vec<(String, ProductVariation)>> {
    let rows = sqlx::query(
        "SELECT product_id, id, name, name_en, name_es, price_centavos, position, is_available
         FROM product_variations WHERE establishment_id = ? AND (? IS NULL OR product_id = ?)
         ORDER BY product_id, position, id",
    )
    .bind(est_id)
    .bind(product_id)
    .bind(product_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get("product_id"),
                ProductVariation {
                    id: row.get("id"),
                    name: row.get("name"),
                    name_en: row.get("name_en"),
                    name_es: row.get("name_es"),
                    price_centavos: row.get("price_centavos"),
                    position: row.get("position"),
                    is_available: row.get("is_available"),
                },
            )
        })
        .collect())
}

pub async fn save(
    pool: &SqlitePool,
    est_id: &str,
    product_id: &str,
    variation: &ProductVariation,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let saved = sqlx::query(
        "INSERT INTO product_variations (id, establishment_id, product_id, name, name_en, name_es,
                                         price_centavos, position, is_available)
         SELECT ?, p.establishment_id, p.id, ?, ?, ?, ?, ?, ? FROM products p
         WHERE p.id = ? AND p.establishment_id = ?
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, name_en = excluded.name_en,
              name_es = excluded.name_es, price_centavos = excluded.price_centavos,
              position = excluded.position, is_available = excluded.is_available
         WHERE product_variations.product_id = excluded.product_id
           AND product_variations.establishment_id = excluded.establishment_id",
    )
    .bind(&variation.id)
    .bind(&variation.name)
    .bind(&variation.name_en)
    .bind(&variation.name_es)
    .bind(variation.price_centavos)
    .bind(variation.position)
    .bind(variation.is_available)
    .bind(product_id)
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
    product_id: &str,
    id: &str,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let deleted = sqlx::query(
        "DELETE FROM product_variations WHERE id = ? AND establishment_id = ? AND product_id = ?",
    )
    .bind(id)
    .bind(est_id)
    .bind(product_id)
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        return Ok(false);
    }
    mark_menu_draft_changed(&mut tx, est_id, now).await?;
    tx.commit().await?;
    Ok(true)
}
