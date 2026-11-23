use askama::Template;

use crate::domain::ocr::{OcrConfidence, OcrImportRecord, OcrImportStatus};
use crate::domain::product::format_price_input;

use super::PageMeta;

pub struct OcrImportSummaryRow {
    pub id: String,
    pub source_filename: String,
    pub source_kind_key: &'static str,
    pub status_key: &'static str,
    pub status_badge_class: &'static str,
    pub items_detected: i64,
    pub items_imported: i64,
    pub page_count: i64,
    pub created_at: String,
}

pub struct OcrReviewRowView {
    pub idx: usize,
    pub include: bool,
    pub category_name: String,
    pub name: String,
    pub price_value: String,
    pub description: String,
    pub is_available: bool,
    pub confidence_key: &'static str,
    pub confidence_badge_class: &'static str,
    pub missing_price: bool,
}

pub fn summary_rows_from_records(list: Vec<OcrImportRecord>) -> Vec<OcrImportSummaryRow> {
    list.into_iter()
        .map(|rec| {
            let status_badge_class = match rec.status {
                OcrImportStatus::Review => "badge-warn",
                OcrImportStatus::Applied => "badge-success",
                OcrImportStatus::Discarded => "badge-neutral",
            };
            OcrImportSummaryRow {
                id: rec.id,
                source_filename: rec.source_filename,
                source_kind_key: rec.source_kind.i18n_key(),
                status_key: rec.status.i18n_key(),
                status_badge_class,
                items_detected: rec.items_detected,
                items_imported: rec.items_imported,
                page_count: rec.page_count,
                created_at: rec.created_at,
            }
        })
        .collect()
}

pub fn rows_from_record(rec: &OcrImportRecord) -> Vec<OcrReviewRowView> {
    rec.draft
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let (price_value, missing_price) = match item.price_centavos {
                Some(c) => (format_price_input(c), false),
                None => (String::new(), true),
            };
            let confidence_badge_class = match item.confidence {
                OcrConfidence::High => "badge-success",
                OcrConfidence::Medium => "badge-warn",
                OcrConfidence::Low => "badge-danger",
            };
            OcrReviewRowView {
                idx,
                include: true,
                category_name: item.category_name.clone(),
                name: item.name.clone(),
                price_value,
                description: item.description.clone(),
                is_available: item.is_available,
                confidence_key: item.confidence.i18n_key(),
                confidence_badge_class,
                missing_price,
            }
        })
        .collect()
}

#[derive(Template)]
#[template(path = "dashboard/ocr_index.html")]
pub struct OcrIndexTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub imports: Vec<OcrImportSummaryRow>,
    pub error_key: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard/ocr_review.html")]
pub struct OcrReviewTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub import_id: String,
    pub source_filename: String,
    pub source_kind_key: &'static str,
    pub status_key: &'static str,
    pub is_editable: bool,
    pub engine_info: String,
    pub page_count: i64,
    pub items_detected: i64,
    pub items_imported: i64,
    pub selected_count: usize,
    pub warnings: Vec<String>,
    pub raw_text: String,
    pub category_suggestions: Vec<String>,
    pub rows: Vec<OcrReviewRowView>,
    pub next_row_idx: usize,
    pub reuse_existing_categories: bool,
    pub mark_visible: bool,
    pub error_key: Option<String>,
}
