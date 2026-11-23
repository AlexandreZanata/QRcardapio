use super::category::Category;
use super::establishment::Establishment;
use super::product::Product;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Logo,
    ProductImage,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::ProductImage => "product_image",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        if raw == "logo" {
            Self::Logo
        } else {
            Self::ProductImage
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaAsset {
    pub id: String,
    pub establishment_id: String,
    pub kind: MediaKind,
    pub content_hash: String,
    pub mime_type: String,
    pub width: i64,
    pub height: i64,
    pub byte_size: i64,
    pub filename_large: String,
    pub filename_thumb: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct PublicationRecord {
    pub id: String,
    pub establishment_id: String,
    pub version: i64,
    pub status: String,
    pub html_bytes: i64,
    pub asset_count: i64,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub published_at: String,
}

#[derive(Debug, Clone)]
pub struct SnapshotCategory {
    pub category: Category,
    pub products: Vec<SnapshotProduct>,
}

#[derive(Debug, Clone)]
pub struct SnapshotProduct {
    pub product: Product,
    pub image: Option<MediaAsset>,
}

#[derive(Debug, Clone)]
pub struct MenuSnapshot {
    pub establishment: Establishment,
    pub logo: Option<MediaAsset>,
    pub categories: Vec<SnapshotCategory>,
}
