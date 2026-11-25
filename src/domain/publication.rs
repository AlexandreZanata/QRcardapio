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
    pub choices: Vec<MenuItemChoice>,
}

#[derive(Debug, Clone)]
pub struct MenuItemChoice {
    pub name: (&'static str, &'static str, &'static str),
    /// A price is present on extras. A choice without a price is a removable ingredient.
    pub price_centavos: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MenuSnapshot {
    pub establishment: Establishment,
    pub logo: Option<MediaAsset>,
    pub categories: Vec<SnapshotCategory>,
}

impl MenuSnapshot {
    pub fn cover_image(&self) -> Option<&MediaAsset> {
        self.categories
            .iter()
            .filter(|section| section.category.is_visible)
            .flat_map(|section| &section.products)
            .filter(|item| item.product.is_visible && item.product.is_available)
            .find_map(|item| item.image.as_ref())
    }
}
