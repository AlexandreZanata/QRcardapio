use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::establishment::{sanitize_optional_text, validate_non_empty_name};
use crate::domain::ocr::{
    ApplyOcrItemInput, ApplyOcrOptions, ApplyOcrValidatedItem, OcrImportRecord, OcrImportStatus,
    OcrSourceKind,
};
use crate::domain::ocr_parser::parse_menu_text;
use crate::domain::product::parse_price_centavos;
use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale};
use crate::infrastructure::database::queries::{
    apply_ocr_import_tx, find_ocr_import_for_est, insert_ocr_import, list_ocr_imports_for_est,
    mark_ocr_import_discarded_for_est,
};
use crate::infrastructure::ocr::OcrService;

const MAX_PASTED_TEXT_CHARS: usize = 60_000;
const MAX_IMPORT_ITEMS: usize = 400;

/// Extracts text from an uploaded photo or PDF, parses structured menu items,
/// and saves a reviewable staging import scoped strictly to `est_id`.
pub async fn extract_from_upload(
    pool: &SqlitePool,
    ocr: &OcrService,
    est_id: &str,
    original_filename: &str,
    raw_bytes: Vec<u8>,
    locale: Locale,
) -> AppResult<OcrImportRecord> {
    let extraction = ocr.extract_file(raw_bytes).await?;
    let default_cat = Catalog::global().t(locale, "ocr.default_category");
    let draft = parse_menu_text(&extraction.raw_text, default_cat);

    let mut warnings = extraction.warning_keys;
    for w in &draft.warning_keys {
        if !warnings.contains(w) {
            warnings.push(w.clone());
        }
    }

    let clean_filename = sanitize_filename_display(original_filename);
    let rec = OcrImportRecord {
        id: Uuid::now_v7().to_string(),
        establishment_id: est_id.to_string(),
        source_filename: clean_filename,
        source_kind: extraction.source_kind,
        status: OcrImportStatus::Review,
        engine_info: extraction.engine_info,
        page_count: i64::from(extraction.page_count),
        raw_text: extraction.raw_text,
        items_detected: draft.items.len() as i64,
        items_imported: 0,
        draft,
        warnings,
        created_at: Utc::now().to_rfc3339(),
        applied_at: None,
    };

    insert_ocr_import(pool, &rec).await?;
    Ok(rec)
}

/// Runs the exact same menu parser and staging review flow on owner-pasted text.
pub async fn extract_from_pasted_text(
    pool: &SqlitePool,
    est_id: &str,
    raw_text: &str,
    locale: Locale,
) -> AppResult<OcrImportRecord> {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("error.ocr_empty_text"));
    }
    let clipped: String = trimmed.chars().take(MAX_PASTED_TEXT_CHARS).collect();
    let default_cat = Catalog::global().t(locale, "ocr.default_category");
    let draft = parse_menu_text(&clipped, default_cat);
    let warnings = draft.warning_keys.clone();

    let rec = OcrImportRecord {
        id: Uuid::now_v7().to_string(),
        establishment_id: est_id.to_string(),
        source_filename: "texto-colado.txt".to_string(),
        source_kind: OcrSourceKind::Text,
        status: OcrImportStatus::Review,
        engine_info: "parser".to_string(),
        page_count: 1,
        raw_text: clipped,
        items_detected: draft.items.len() as i64,
        items_imported: 0,
        draft,
        warnings,
        created_at: Utc::now().to_rfc3339(),
        applied_at: None,
    };

    insert_ocr_import(pool, &rec).await?;
    Ok(rec)
}

pub async fn get_import(
    pool: &SqlitePool,
    est_id: &str,
    import_id: &str,
) -> AppResult<OcrImportRecord> {
    find_ocr_import_for_est(pool, est_id, import_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn list_recent_imports(
    pool: &SqlitePool,
    est_id: &str,
) -> AppResult<Vec<OcrImportRecord>> {
    list_ocr_imports_for_est(pool, est_id, 15).await
}

pub async fn discard_import(pool: &SqlitePool, est_id: &str, import_id: &str) -> AppResult<()> {
    let existing = find_ocr_import_for_est(pool, est_id, import_id).await?;
    if existing.is_none() {
        return Err(AppError::NotFound("error.not_found"));
    }
    let _ = mark_ocr_import_discarded_for_est(pool, est_id, import_id).await?;
    Ok(())
}

/// Validates owner-edited review items strictly (integer centavos, non-empty names/categories)
/// and commits them transactionally into the establishment's draft menu.
pub async fn apply_import(
    pool: &SqlitePool,
    est_id: &str,
    import_id: &str,
    submitted: &[ApplyOcrItemInput],
    opts: ApplyOcrOptions,
    locale: Locale,
) -> AppResult<usize> {
    if submitted.is_empty() {
        return Err(AppError::Validation("error.ocr_no_selected_items"));
    }
    if submitted.len() > MAX_IMPORT_ITEMS {
        return Err(AppError::Validation("error.ocr_too_many_items"));
    }

    let default_cat = Catalog::global().t(locale, "ocr.default_category");
    let mut validated = Vec::with_capacity(submitted.len());

    for item in submitted {
        let name = validate_non_empty_name(&item.name, 100)?;
        let cat_raw = if item.category_name.trim().is_empty() {
            default_cat
        } else {
            item.category_name.as_str()
        };
        let category_name = validate_non_empty_name(cat_raw, 80)?;
        let price_centavos = parse_price_centavos(&item.price_raw)?;
        let description = sanitize_optional_text(&item.description, 320);

        validated.push(ApplyOcrValidatedItem {
            category_name,
            name,
            description,
            price_centavos,
            is_available: item.is_available,
        });
    }

    let now = Utc::now().to_rfc3339();
    apply_ocr_import_tx(pool, est_id, import_id, &validated, opts, &now).await
}

fn sanitize_filename_display(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("cardapio").trim();
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(120).collect();
    if cleaned.is_empty() {
        "cardapio".to_string()
    } else {
        cleaned
    }
}
