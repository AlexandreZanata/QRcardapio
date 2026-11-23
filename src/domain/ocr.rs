use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrSourceKind {
    Image,
    PdfText,
    PdfOcr,
    Text,
}

impl OcrSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::PdfText => "pdf_text",
            Self::PdfOcr => "pdf_ocr",
            Self::Text => "text",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "image" => Self::Image,
            "pdf_text" => Self::PdfText,
            "pdf_ocr" => Self::PdfOcr,
            _ => Self::Text,
        }
    }

    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Image => "ocr.source_image",
            Self::PdfText => "ocr.source_pdf_text",
            Self::PdfOcr => "ocr.source_pdf_ocr",
            Self::Text => "ocr.source_text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrImportStatus {
    Review,
    Applied,
    Discarded,
}

impl OcrImportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Applied => "applied",
            Self::Discarded => "discarded",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "applied" => Self::Applied,
            "discarded" => Self::Discarded,
            _ => Self::Review,
        }
    }

    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Review => "ocr.status_review",
            Self::Applied => "ocr.status_applied",
            Self::Discarded => "ocr.status_discarded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrConfidence {
    High,
    Medium,
    Low,
}

impl OcrConfidence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::High => "ocr.conf_high",
            Self::Medium => "ocr.conf_medium",
            Self::Low => "ocr.conf_low",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMenuItem {
    pub category_name: String,
    pub name: String,
    pub description: String,
    pub price_centavos: Option<i64>,
    pub confidence: OcrConfidence,
    pub is_available: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedMenuDraft {
    pub categories: Vec<String>,
    pub items: Vec<ParsedMenuItem>,
    pub warning_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OcrImportRecord {
    pub id: String,
    pub establishment_id: String,
    pub source_filename: String,
    pub source_kind: OcrSourceKind,
    pub status: OcrImportStatus,
    pub engine_info: String,
    pub page_count: i64,
    pub raw_text: String,
    pub draft: ParsedMenuDraft,
    pub warnings: Vec<String>,
    pub items_detected: i64,
    pub items_imported: i64,
    pub created_at: String,
    pub applied_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApplyOcrItemInput {
    pub category_name: String,
    pub name: String,
    pub description: String,
    pub price_raw: String,
    pub is_available: bool,
}

#[derive(Debug, Clone)]
pub struct ApplyOcrValidatedItem {
    pub category_name: String,
    pub name: String,
    pub description: String,
    pub price_centavos: i64,
    pub is_available: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ApplyOcrOptions {
    pub reuse_existing_categories: bool,
    pub mark_visible: bool,
}
