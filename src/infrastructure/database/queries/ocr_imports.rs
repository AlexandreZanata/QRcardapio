use std::collections::HashMap;

use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::domain::ocr::{
    ApplyOcrOptions, ApplyOcrValidatedItem, OcrImportRecord, OcrImportStatus, OcrSourceKind,
    ParsedMenuDraft,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn insert_ocr_import(pool: &SqlitePool, rec: &OcrImportRecord) -> AppResult<()> {
    let parsed_json = serde_json::to_string(&rec.draft)
        .map_err(|e| AppError::Internal(format!("serialize ocr draft: {e}")))?;
    let warnings_json = serde_json::to_string(&rec.warnings)
        .map_err(|e| AppError::Internal(format!("serialize ocr warnings: {e}")))?;

    sqlx::query(
        "INSERT INTO ocr_imports (
            id, establishment_id, source_filename, source_kind, status,
            engine_info, page_count, raw_text, parsed_json, warnings_json,
            items_detected, items_imported, created_at, applied_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&rec.id)
    .bind(&rec.establishment_id)
    .bind(&rec.source_filename)
    .bind(rec.source_kind.as_str())
    .bind(rec.status.as_str())
    .bind(&rec.engine_info)
    .bind(rec.page_count)
    .bind(&rec.raw_text)
    .bind(&parsed_json)
    .bind(&warnings_json)
    .bind(rec.items_detected)
    .bind(rec.items_imported)
    .bind(&rec.created_at)
    .bind(&rec.applied_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_ocr_import_for_est(
    pool: &SqlitePool,
    est_id: &str,
    import_id: &str,
) -> AppResult<Option<OcrImportRecord>> {
    let row = sqlx::query("SELECT * FROM ocr_imports WHERE id = ? AND establishment_id = ?")
        .bind(import_id)
        .bind(est_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_ocr_import(&r)))
}

pub async fn list_ocr_imports_for_est(
    pool: &SqlitePool,
    est_id: &str,
    limit: i64,
) -> AppResult<Vec<OcrImportRecord>> {
    let rows = sqlx::query(
        "SELECT * FROM ocr_imports
         WHERE establishment_id = ?
         ORDER BY created_at DESC
         LIMIT ?",
    )
    .bind(est_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_ocr_import).collect())
}

pub async fn mark_ocr_import_discarded_for_est(
    pool: &SqlitePool,
    est_id: &str,
    import_id: &str,
) -> AppResult<bool> {
    let res = sqlx::query(
        "UPDATE ocr_imports
         SET status = 'discarded'
         WHERE id = ? AND establishment_id = ? AND status = 'review'",
    )
    .bind(import_id)
    .bind(est_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Atomically creates missing categories, inserts validated products, flags unpublished changes,
/// and marks the OCR import record as `applied` inside a single `BEGIN IMMEDIATE` transaction.
pub async fn apply_ocr_import_tx(
    pool: &SqlitePool,
    est_id: &str,
    import_id: &str,
    items: &[ApplyOcrValidatedItem],
    opts: ApplyOcrOptions,
    now: &str,
) -> AppResult<usize> {
    let mut tx = begin_immediate(pool).await?;

    let status_row: Option<String> =
        sqlx::query_scalar("SELECT status FROM ocr_imports WHERE id = ? AND establishment_id = ?")
            .bind(import_id)
            .bind(est_id)
            .fetch_optional(&mut *tx)
            .await?;

    let Some(status) = status_row else {
        return Err(AppError::NotFound("error.not_found"));
    };
    if status == OcrImportStatus::Applied.as_str() {
        return Err(AppError::Conflict("error.ocr_already_applied"));
    }

    let existing_cats = sqlx::query(
        "SELECT id, name, position FROM categories
         WHERE establishment_id = ?
         ORDER BY position ASC, id ASC",
    )
    .bind(est_id)
    .fetch_all(&mut *tx)
    .await?;

    let mut next_cat_pos: i64 = 0;
    let mut cat_lookup: HashMap<String, String> = HashMap::new();
    for r in &existing_cats {
        let cid: String = r.get("id");
        let cname: String = r.get("name");
        let pos: i64 = r.get("position");
        if pos > next_cat_pos {
            next_cat_pos = pos;
        }
        if opts.reuse_existing_categories {
            cat_lookup.entry(cname.trim().to_lowercase()).or_insert(cid);
        }
    }

    let mut prod_next_pos: HashMap<String, i64> = HashMap::new();

    for item in items {
        let cat_key = item.category_name.trim().to_lowercase();
        let category_id = if let Some(existing_id) = cat_lookup.get(&cat_key) {
            existing_id.clone()
        } else {
            next_cat_pos += 1;
            let new_cat_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO categories (
                    id, establishment_id, name, name_en, name_es,
                    description, description_en, description_es,
                    position, is_visible, created_at, updated_at
                 ) VALUES (?, ?, ?, '', '', '', '', '', ?, ?, ?, ?)",
            )
            .bind(&new_cat_id)
            .bind(est_id)
            .bind(&item.category_name)
            .bind(next_cat_pos)
            .bind(i64::from(opts.mark_visible))
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            cat_lookup.insert(cat_key, new_cat_id.clone());
            prod_next_pos.insert(new_cat_id.clone(), 0);
            new_cat_id
        };

        let next_pos = if let Some(pos) = prod_next_pos.get_mut(&category_id) {
            *pos += 1;
            *pos
        } else {
            let max_pos: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(position), 0)
                 FROM products WHERE establishment_id = ? AND category_id = ?",
            )
            .bind(est_id)
            .bind(&category_id)
            .fetch_one(&mut *tx)
            .await?;
            let p = max_pos + 1;
            prod_next_pos.insert(category_id.clone(), p);
            p
        };

        let prod_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO products (
                id, establishment_id, category_id, name, name_en, name_es,
                description, description_en, description_es, price_centavos,
                image_asset_id, position, is_available, is_visible, created_at, updated_at
             ) VALUES (?, ?, ?, ?, '', '', ?, '', '', ?, NULL, ?, ?, ?, ?, ?)",
        )
        .bind(&prod_id)
        .bind(est_id)
        .bind(&category_id)
        .bind(&item.name)
        .bind(&item.description)
        .bind(item.price_centavos)
        .bind(next_pos)
        .bind(i64::from(item.is_available))
        .bind(i64::from(opts.mark_visible))
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    let imported_count = items.len();
    sqlx::query(
        "UPDATE ocr_imports
         SET status = 'applied', items_imported = ?, applied_at = ?
         WHERE id = ? AND establishment_id = ?",
    )
    .bind(imported_count as i64)
    .bind(now)
    .bind(import_id)
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
    Ok(imported_count)
}

fn row_to_ocr_import(r: &SqliteRow) -> OcrImportRecord {
    let parsed_raw: String = r.get("parsed_json");
    let warnings_raw: String = r.get("warnings_json");
    let draft: ParsedMenuDraft = serde_json::from_str(&parsed_raw).unwrap_or_default();
    let warnings: Vec<String> = serde_json::from_str(&warnings_raw).unwrap_or_default();

    OcrImportRecord {
        id: r.get("id"),
        establishment_id: r.get("establishment_id"),
        source_filename: r.get("source_filename"),
        source_kind: OcrSourceKind::from_db(r.get::<String, _>("source_kind").as_str()),
        status: OcrImportStatus::from_db(r.get::<String, _>("status").as_str()),
        engine_info: r.get("engine_info"),
        page_count: r.get("page_count"),
        raw_text: r.get("raw_text"),
        draft,
        warnings,
        items_detected: r.get("items_detected"),
        items_imported: r.get("items_imported"),
        created_at: r.get("created_at"),
        applied_at: r.get("applied_at"),
    }
}
