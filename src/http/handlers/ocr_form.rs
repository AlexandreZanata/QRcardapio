use std::collections::{HashMap, HashSet};

use crate::app::AppState;
use crate::domain::account::AccountRole;
use crate::domain::ocr::{OcrConfidence, OcrImportRecord, OcrImportStatus};
use crate::error::AppResult;
use crate::http::middleware::auth::TenantContext;
use crate::i18n::Locale;
use crate::infrastructure::database::queries::list_categories_for_est;
use crate::rendering::ocr::{OcrReviewRowView, OcrReviewTemplate};
use crate::rendering::{FlashMessage, PageMeta};

pub(super) struct ParsedApplyForm {
    pub csrf: String,
    pub reuse_existing_categories: bool,
    pub mark_visible: bool,
    pub rows: Vec<OcrReviewRowView>,
}

pub(super) fn parse_apply_form_pairs(pairs: Vec<(String, String)>) -> ParsedApplyForm {
    let mut csrf = String::new();
    let mut reuse_existing = false;
    let mut mark_visible = false;
    let mut row_order: Vec<usize> = Vec::new();
    let mut fields: HashMap<String, String> = HashMap::new();

    for (k, v) in pairs {
        match k.as_str() {
            "_csrf" => csrf = v,
            "reuse_existing_categories" => reuse_existing = v == "1" || v == "on",
            "mark_visible" => mark_visible = v == "1" || v == "on",
            "row_idx" => {
                if let Ok(idx) = v.parse::<usize>()
                    && !row_order.contains(&idx)
                {
                    row_order.push(idx);
                }
            }
            _ => {
                fields.insert(k, v);
            }
        }
    }

    let rows = row_order
        .into_iter()
        .map(|idx| {
            let include = fields.contains_key(&format!("include_{idx}"));
            let category_name = fields
                .remove(&format!("category_{idx}"))
                .unwrap_or_default();
            let name = fields.remove(&format!("name_{idx}")).unwrap_or_default();
            let price_value = fields.remove(&format!("price_{idx}")).unwrap_or_default();
            let description = fields
                .remove(&format!("description_{idx}"))
                .unwrap_or_default();
            let is_available = fields.contains_key(&format!("available_{idx}"));
            let missing_price = price_value.trim().is_empty();

            OcrReviewRowView {
                idx,
                include,
                category_name,
                name,
                price_value,
                description,
                is_available,
                confidence_key: if missing_price {
                    OcrConfidence::Low.i18n_key()
                } else {
                    OcrConfidence::High.i18n_key()
                },
                confidence_badge_class: if missing_price {
                    "badge-danger"
                } else {
                    "badge-success"
                },
                missing_price,
            }
        })
        .collect();

    ParsedApplyForm {
        csrf,
        reuse_existing_categories: reuse_existing,
        mark_visible,
        rows,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn build_review_template(
    state: &AppState,
    locale: Locale,
    tenant: &TenantContext,
    rec: &OcrImportRecord,
    rows: Vec<OcrReviewRowView>,
    reuse_existing_categories: bool,
    mark_visible: bool,
    flash: Option<FlashMessage>,
    error_key: Option<String>,
) -> AppResult<OcrReviewTemplate> {
    let existing_cats = list_categories_for_est(&state.pool, &tenant.establishment.id).await?;
    let mut seen = HashSet::new();
    let mut suggestions = Vec::new();
    for c in existing_cats {
        if seen.insert(c.name.to_lowercase()) {
            suggestions.push(c.name);
        }
    }
    for c in &rec.draft.categories {
        if seen.insert(c.to_lowercase()) {
            suggestions.push(c.clone());
        }
    }

    let next_row_idx = rows.iter().map(|r| r.idx + 1).max().unwrap_or(0);
    let selected_count = rows.iter().filter(|r| r.include).count();

    Ok(OcrReviewTemplate {
        page: PageMeta::new(locale, "ocr.review_page_title", flash),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "ocr",
        csrf_token: tenant.auth.csrf_token.clone(),
        import_id: rec.id.clone(),
        source_filename: rec.source_filename.clone(),
        source_kind_key: rec.source_kind.i18n_key(),
        status_key: rec.status.i18n_key(),
        is_editable: rec.status == OcrImportStatus::Review,
        engine_info: rec.engine_info.clone(),
        page_count: rec.page_count,
        items_detected: rec.items_detected,
        items_imported: rec.items_imported,
        selected_count,
        warnings: rec.warnings.clone(),
        raw_text: rec.raw_text.clone(),
        category_suggestions: suggestions,
        rows,
        next_row_idx,
        reuse_existing_categories,
        mark_visible,
        error_key,
    })
}
