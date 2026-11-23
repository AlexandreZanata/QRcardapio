use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::category::Category;
use crate::domain::establishment::{sanitize_optional_text, validate_non_empty_name};
use crate::domain::product::{Product, parse_price_centavos};
use crate::domain::publication::MediaKind;
use crate::error::{AppError, AppResult};
use crate::infrastructure::assets::AssetProcessor;
use crate::infrastructure::database::queries::{
    delete_category_for_est, delete_media_asset_for_est, delete_product_for_est,
    find_category_for_est, find_media_asset_for_est, find_product_for_est, insert_category,
    insert_media_asset, insert_product, list_products_for_est, move_category_for_est,
    move_product_for_est, toggle_product_availability_for_est, update_category_for_est,
    update_product_for_est,
};

pub struct CategoryInput<'a> {
    pub name: &'a str,
    pub name_en: &'a str,
    pub name_es: &'a str,
    pub description: &'a str,
    pub description_en: &'a str,
    pub description_es: &'a str,
    pub is_visible: bool,
}

pub struct ProductInput<'a> {
    pub category_id: &'a str,
    pub name: &'a str,
    pub name_en: &'a str,
    pub name_es: &'a str,
    pub description: &'a str,
    pub description_en: &'a str,
    pub description_es: &'a str,
    pub price_raw: &'a str,
    pub is_available: bool,
    pub is_visible: bool,
}

pub async fn create_category(
    pool: &SqlitePool,
    est_id: &str,
    input: CategoryInput<'_>,
) -> AppResult<Category> {
    let name = validate_non_empty_name(input.name, 80)?;
    let now = Utc::now().to_rfc3339();
    let cat = Category {
        id: Uuid::now_v7().to_string(),
        establishment_id: est_id.to_string(),
        name,
        name_en: sanitize_optional_text(input.name_en, 80),
        name_es: sanitize_optional_text(input.name_es, 80),
        description: sanitize_optional_text(input.description, 240),
        description_en: sanitize_optional_text(input.description_en, 240),
        description_es: sanitize_optional_text(input.description_es, 240),
        position: 0,
        is_visible: input.is_visible,
        created_at: now.clone(),
        updated_at: now,
    };
    insert_category(pool, &cat).await?;
    find_category_for_est(pool, est_id, &cat.id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn update_category(
    pool: &SqlitePool,
    est_id: &str,
    cat_id: &str,
    input: CategoryInput<'_>,
) -> AppResult<()> {
    let name = validate_non_empty_name(input.name, 80)?;
    let now = Utc::now().to_rfc3339();
    let updated = update_category_for_est(
        pool,
        est_id,
        cat_id,
        &name,
        &sanitize_optional_text(input.name_en, 80),
        &sanitize_optional_text(input.name_es, 80),
        &sanitize_optional_text(input.description, 240),
        &sanitize_optional_text(input.description_en, 240),
        &sanitize_optional_text(input.description_es, 240),
        input.is_visible,
        &now,
    )
    .await?;

    if !updated {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

pub async fn delete_category(
    pool: &SqlitePool,
    assets: &AssetProcessor,
    est_id: &str,
    cat_id: &str,
) -> AppResult<()> {
    // Capture product media IDs inside this category so we can remove orphaned files on disk.
    let products = list_products_for_est(pool, est_id).await?;
    let mut orphaned_assets = Vec::new();
    for p in products.into_iter().filter(|p| p.category_id == cat_id) {
        if let Some(asset_id) = p.image_asset_id
            && let Some(asset) = find_media_asset_for_est(pool, est_id, &asset_id).await?
        {
            orphaned_assets.push(asset);
        }
    }

    let now = Utc::now().to_rfc3339();
    if !delete_category_for_est(pool, est_id, cat_id, &now).await? {
        return Err(AppError::NotFound("error.not_found"));
    }

    for asset in orphaned_assets {
        let _ = delete_media_asset_for_est(pool, est_id, &asset.id).await;
        assets.remove_files(&asset);
    }
    Ok(())
}

pub async fn move_category(
    pool: &SqlitePool,
    est_id: &str,
    cat_id: &str,
    move_up: bool,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    if !move_category_for_est(pool, est_id, cat_id, move_up, &now).await? {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

pub async fn create_product(
    pool: &SqlitePool,
    assets: &AssetProcessor,
    est_id: &str,
    input: ProductInput<'_>,
    image_bytes: Option<Vec<u8>>,
) -> AppResult<Product> {
    let name = validate_non_empty_name(input.name, 100)?;
    let price_centavos = parse_price_centavos(input.price_raw)?;

    if find_category_for_est(pool, est_id, input.category_id)
        .await?
        .is_none()
    {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    let image_asset = match image_bytes {
        Some(bytes) if !bytes.is_empty() => {
            let (asset, _, _) = assets
                .process_and_store(est_id, MediaKind::ProductImage, bytes)
                .await?;
            insert_media_asset(pool, &asset).await?;
            Some(asset)
        }
        _ => None,
    };

    let now = Utc::now().to_rfc3339();
    let prod = Product {
        id: Uuid::now_v7().to_string(),
        establishment_id: est_id.to_string(),
        category_id: input.category_id.to_string(),
        name,
        name_en: sanitize_optional_text(input.name_en, 100),
        name_es: sanitize_optional_text(input.name_es, 100),
        description: sanitize_optional_text(input.description, 320),
        description_en: sanitize_optional_text(input.description_en, 320),
        description_es: sanitize_optional_text(input.description_es, 320),
        price_centavos,
        image_asset_id: image_asset.as_ref().map(|a| a.id.clone()),
        position: 0,
        is_available: input.is_available,
        is_visible: input.is_visible,
        created_at: now.clone(),
        updated_at: now,
    };

    if let Err(err) = insert_product(pool, &prod).await {
        if let Some(asset) = image_asset {
            let _ = delete_media_asset_for_est(pool, est_id, &asset.id).await;
            assets.remove_files(&asset);
        }
        return Err(err);
    }

    find_product_for_est(pool, est_id, &prod.id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn update_product(
    pool: &SqlitePool,
    assets: &AssetProcessor,
    est_id: &str,
    prod_id: &str,
    input: ProductInput<'_>,
    new_image: Option<Vec<u8>>,
    remove_image: bool,
) -> AppResult<()> {
    let existing = find_product_for_est(pool, est_id, prod_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let name = validate_non_empty_name(input.name, 100)?;
    let price_centavos = parse_price_centavos(input.price_raw)?;

    if find_category_for_est(pool, est_id, input.category_id)
        .await?
        .is_none()
    {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    let mut next_asset_id = existing.image_asset_id.clone();
    let mut asset_to_cleanup: Option<String> = None;

    if let Some(bytes) = new_image
        && !bytes.is_empty()
    {
        let (new_asset, _, _) = assets
            .process_and_store(est_id, MediaKind::ProductImage, bytes)
            .await?;
        insert_media_asset(pool, &new_asset).await?;
        asset_to_cleanup = existing.image_asset_id.clone();
        next_asset_id = Some(new_asset.id);
    } else if remove_image {
        asset_to_cleanup = existing.image_asset_id.clone();
        next_asset_id = None;
    }

    let now = Utc::now().to_rfc3339();
    let ok = update_product_for_est(
        pool,
        est_id,
        prod_id,
        input.category_id,
        &name,
        &sanitize_optional_text(input.name_en, 100),
        &sanitize_optional_text(input.name_es, 100),
        &sanitize_optional_text(input.description, 320),
        &sanitize_optional_text(input.description_en, 320),
        &sanitize_optional_text(input.description_es, 320),
        price_centavos,
        next_asset_id.as_deref(),
        input.is_available,
        input.is_visible,
        &now,
    )
    .await?;

    if !ok {
        return Err(AppError::NotFound("error.not_found"));
    }

    if let Some(old_id) = asset_to_cleanup
        && let Some(old_asset) = find_media_asset_for_est(pool, est_id, &old_id).await?
    {
        let _ = delete_media_asset_for_est(pool, est_id, &old_asset.id).await;
        assets.remove_files(&old_asset);
    }

    Ok(())
}

pub async fn toggle_product_availability(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
) -> AppResult<bool> {
    let now = Utc::now().to_rfc3339();
    toggle_product_availability_for_est(pool, est_id, prod_id, &now)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn delete_product(
    pool: &SqlitePool,
    assets: &AssetProcessor,
    est_id: &str,
    prod_id: &str,
) -> AppResult<()> {
    let existing = find_product_for_est(pool, est_id, prod_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let now = Utc::now().to_rfc3339();
    if !delete_product_for_est(pool, est_id, prod_id, &now).await? {
        return Err(AppError::NotFound("error.not_found"));
    }

    if let Some(asset_id) = existing.image_asset_id
        && let Some(asset) = find_media_asset_for_est(pool, est_id, &asset_id).await?
    {
        let _ = delete_media_asset_for_est(pool, est_id, &asset.id).await;
        assets.remove_files(&asset);
    }

    Ok(())
}

pub async fn move_product(
    pool: &SqlitePool,
    est_id: &str,
    prod_id: &str,
    move_up: bool,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    if !move_product_for_est(pool, est_id, prod_id, move_up, &now).await? {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}
