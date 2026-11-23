use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::domain::account::{Account, AccountRole};
use crate::domain::category::Category;
use crate::domain::establishment::Establishment;
use crate::domain::product::Product;
use crate::domain::publication::{MediaAsset, MediaKind};
use crate::i18n::Locale;

pub(crate) fn row_to_account(r: &SqliteRow) -> Account {
    Account {
        id: r.get("id"),
        email: r.get("email"),
        password_hash: r.get("password_hash"),
        role: AccountRole::from_db(r.get::<String, _>("role").as_str()),
        preferred_locale: Locale::parse(r.get::<String, _>("preferred_locale").as_str())
            .unwrap_or(Locale::PtBr),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub(crate) fn row_to_establishment(r: &SqliteRow) -> Establishment {
    Establishment {
        id: r.get("id"),
        slug: r.get("slug"),
        name: r.get("name"),
        description: r.get("description"),
        description_en: r.get("description_en"),
        description_es: r.get("description_es"),
        accent_color: r.get("accent_color"),
        logo_asset_id: r.get("logo_asset_id"),
        google_maps_url: r.get("google_maps_url"),
        google_review_url: r.get("google_review_url"),
        is_published: r.get::<i64, _>("is_published") == 1,
        published_version: r.get("published_version"),
        has_unpublished_changes: r.get::<i64, _>("has_unpublished_changes") == 1,
        last_published_at: r.get("last_published_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub(crate) fn row_to_category(r: &SqliteRow) -> Category {
    Category {
        id: r.get("id"),
        establishment_id: r.get("establishment_id"),
        name: r.get("name"),
        name_en: r.get("name_en"),
        name_es: r.get("name_es"),
        description: r.get("description"),
        description_en: r.get("description_en"),
        description_es: r.get("description_es"),
        position: r.get("position"),
        is_visible: r.get::<i64, _>("is_visible") == 1,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub(crate) fn row_to_product(r: &SqliteRow) -> Product {
    Product {
        id: r.get("id"),
        establishment_id: r.get("establishment_id"),
        category_id: r.get("category_id"),
        name: r.get("name"),
        name_en: r.get("name_en"),
        name_es: r.get("name_es"),
        description: r.get("description"),
        description_en: r.get("description_en"),
        description_es: r.get("description_es"),
        price_centavos: r.get("price_centavos"),
        image_asset_id: r.get("image_asset_id"),
        position: r.get("position"),
        is_available: r.get::<i64, _>("is_available") == 1,
        is_visible: r.get::<i64, _>("is_visible") == 1,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub(crate) fn row_to_media(r: &SqliteRow) -> MediaAsset {
    MediaAsset {
        id: r.get("id"),
        establishment_id: r.get("establishment_id"),
        kind: MediaKind::from_db(r.get::<String, _>("kind").as_str()),
        content_hash: r.get("content_hash"),
        mime_type: r.get("mime_type"),
        width: r.get("width"),
        height: r.get("height"),
        byte_size: r.get("byte_size"),
        filename_large: r.get("filename_large"),
        filename_thumb: r.get("filename_thumb"),
        created_at: r.get("created_at"),
    }
}
