//! Read-only menu for the QR in the landing hero. The catalog belongs to the release.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;

use image::{ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::application::publishing::write_static_snapshot_with_media;
use crate::config::Config;
use crate::domain::category::Category;
use crate::domain::establishment::{Establishment, PUBLIC_DEMO_SLUG};
use crate::domain::product::Product;
use crate::domain::publication::{
    MediaAsset, MediaKind, MenuSnapshot, SnapshotCategory, SnapshotProduct,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::establishment_slug_exists;

mod catalog;
mod choices;
use catalog::{CATEGORIES, DESCRIPTION};

const DEMO_ID: &str = "public-demo-bistro-da-praca";
const SEED_TIME: &str = "2026-11-24T00:00:00Z";
const LOGO: &[u8] = include_bytes!("demo_menu/images/bistro-logo.png");
const COVER: &[u8] = include_bytes!("demo_menu/images/cover.jpg");

/// Rebuilds the demonstration through the same atomic snapshot publisher as customer menus.
pub async fn seed_public_demo(pool: &SqlitePool, config: &Config) -> AppResult<()> {
    if establishment_slug_exists(pool, PUBLIC_DEMO_SLUG).await? {
        return Err(AppError::Internal(format!(
            "public demo address '{PUBLIC_DEMO_SLUG}' belongs to an existing establishment"
        )));
    }

    let mut files = HashMap::new();
    let logo = embedded_image(&mut files, "logo", MediaKind::Logo, LOGO, LOGO)?;
    let mut snapshot = MenuSnapshot {
        establishment: demo_establishment(&logo),
        logo: Some(logo),
        categories: Vec::with_capacity(CATEGORIES.len()),
    };

    for (section_position, section) in CATEGORIES.iter().enumerate() {
        let mut category = SnapshotCategory {
            category: Category {
                id: section.id.into(),
                establishment_id: DEMO_ID.into(),
                name: section.name.0.into(),
                name_en: section.name.1.into(),
                name_es: section.name.2.into(),
                description: section.description.0.into(),
                description_en: section.description.1.into(),
                description_es: section.description.2.into(),
                position: section_position as i64,
                is_visible: true,
                created_at: SEED_TIME.into(),
                updated_at: SEED_TIME.into(),
            },
            products: Vec::with_capacity(section.items.len()),
        };

        for (position, item) in section.items.iter().enumerate() {
            let large = if section_position == 0 && position == 0 {
                COVER
            } else {
                item.image
            };
            let image = embedded_image(
                &mut files,
                item.id,
                MediaKind::ProductImage,
                item.image,
                large,
            )?;
            category.products.push(SnapshotProduct {
                product: Product {
                    id: item.id.into(),
                    establishment_id: DEMO_ID.into(),
                    category_id: section.id.into(),
                    name: item.name.0.into(),
                    name_en: item.name.1.into(),
                    name_es: item.name.2.into(),
                    description: item.description.0.into(),
                    description_en: item.description.1.into(),
                    description_es: item.description.2.into(),
                    price_centavos: item.price_centavos,
                    image_asset_id: Some(image.id.clone()),
                    position: position as i64,
                    is_available: true,
                    is_visible: true,
                    created_at: SEED_TIME.into(),
                    updated_at: SEED_TIME.into(),
                },
                image: Some(image),
                choices: choices::for_product(item.id),
                catalog_options: Default::default(),
            });
        }
        snapshot.categories.push(category);
    }

    write_static_snapshot_with_media(config, &snapshot, |filename| {
        files
            .get(filename)
            .copied()
            .map(Cow::Borrowed)
            .ok_or_else(|| AppError::Internal(format!("missing embedded demo image '{filename}'")))
    })?;
    tracing::info!(slug = PUBLIC_DEMO_SLUG, "public demo snapshot ready");
    Ok(())
}

fn embedded_image(
    files: &mut HashMap<String, &'static [u8]>,
    id: &str,
    kind: MediaKind,
    thumb: &'static [u8],
    large: &'static [u8],
) -> AppResult<MediaAsset> {
    let (format, extension, mime_type) = match kind {
        MediaKind::Logo => (ImageFormat::Png, "png", "image/png"),
        MediaKind::ProductImage => (ImageFormat::Jpeg, "jpg", "image/jpeg"),
    };
    let (width, height) = ImageReader::with_format(Cursor::new(large), format)
        .into_dimensions()
        .map_err(|error| AppError::Internal(format!("invalid demo image '{id}': {error}")))?;
    let content_hash = image_hash(large);
    let filename_large = format!("{content_hash}.{extension}");
    let filename_thumb = format!("{}.{extension}", image_hash(thumb));
    files.insert(filename_large.clone(), large);
    files.insert(filename_thumb.clone(), thumb);
    Ok(MediaAsset {
        id: id.into(),
        establishment_id: DEMO_ID.into(),
        kind,
        content_hash,
        mime_type: mime_type.into(),
        width: i64::from(width),
        height: i64::from(height),
        byte_size: large.len() as i64,
        filename_large,
        filename_thumb,
        created_at: SEED_TIME.into(),
    })
}

fn image_hash(bytes: &[u8]) -> String {
    hex::encode(&Sha256::digest(bytes)[..8])
}

fn demo_establishment(logo: &MediaAsset) -> Establishment {
    Establishment {
        id: DEMO_ID.into(),
        slug: PUBLIC_DEMO_SLUG.into(),
        name: "Bistrô da Praça".into(),
        description: DESCRIPTION.0.into(),
        description_en: DESCRIPTION.1.into(),
        description_es: DESCRIPTION.2.into(),
        accent_color: "#263BE8".into(),
        logo_asset_id: Some(logo.id.clone()),
        google_maps_url: None,
        google_review_url: None,
        is_published: true,
        published_version: 1,
        has_unpublished_changes: false,
        last_published_at: Some(SEED_TIME.into()),
        created_at: SEED_TIME.into(),
        updated_at: SEED_TIME.into(),
    }
}
