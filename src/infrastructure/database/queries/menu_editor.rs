use sqlx::{FromRow, SqlitePool};

use crate::error::AppResult;

const PAGE_SIZE: i64 = 12;

#[derive(FromRow)]
pub struct MenuEditorRow {
    pub category_id: String,
    pub category_name: String,
    pub category_name_en: String,
    pub category_name_es: String,
    pub category_visible: bool,
    pub product_id: Option<String>,
    pub product_name: String,
    pub product_name_en: String,
    pub product_name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub price_centavos: i64,
    pub filename_thumb: Option<String>,
    pub is_available: bool,
    pub is_visible: bool,
}

pub struct MenuEditorPage {
    pub rows: Vec<MenuEditorRow>,
    pub current: i64,
    pub pages: i64,
}

pub async fn list_page(
    pool: &SqlitePool,
    establishment_id: &str,
    requested_page: i64,
) -> AppResult<MenuEditorPage> {
    let mut tx = pool.begin().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM categories c
         LEFT JOIN products p ON p.category_id = c.id AND p.establishment_id = c.establishment_id
         WHERE c.establishment_id = ?",
    )
    .bind(establishment_id)
    .fetch_one(&mut *tx)
    .await?;

    let pages = (1 + count.saturating_sub(1) / PAGE_SIZE).max(1);
    let current = requested_page.clamp(1, pages);
    let rows = sqlx::query_as(
        "SELECT c.id AS category_id, c.name AS category_name,
                c.name_en AS category_name_en, c.name_es AS category_name_es,
                c.is_visible AS category_visible, p.id AS product_id,
                COALESCE(p.name, '') AS product_name,
                COALESCE(p.name_en, '') AS product_name_en,
                COALESCE(p.name_es, '') AS product_name_es,
                COALESCE(p.description, '') AS description,
                COALESCE(p.description_en, '') AS description_en,
                COALESCE(p.description_es, '') AS description_es,
                COALESCE(p.price_centavos, 0) AS price_centavos, m.filename_thumb,
                COALESCE(p.is_available, 0) AS is_available,
                COALESCE(p.is_visible, 0) AS is_visible
         FROM categories c
         LEFT JOIN products p ON p.category_id = c.id AND p.establishment_id = c.establishment_id
         LEFT JOIN media_assets m ON m.id = p.image_asset_id AND m.establishment_id = c.establishment_id
         WHERE c.establishment_id = ?
         ORDER BY c.position, c.id, p.position, p.id
         LIMIT ? OFFSET ?",
    )
    .bind(establishment_id)
    .bind(PAGE_SIZE)
    .bind((current - 1) * PAGE_SIZE)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(MenuEditorPage {
        rows,
        current,
        pages,
    })
}
