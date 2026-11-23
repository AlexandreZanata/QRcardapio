use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::auth::is_unique_violation;
use crate::domain::establishment::{
    AccentColor, Establishment, Slug, sanitize_optional_text, validate_non_empty_name,
};
use crate::domain::publication::{MediaAsset, MediaKind};
use crate::error::{AppError, AppResult};
use crate::infrastructure::assets::AssetProcessor;
use crate::infrastructure::database::queries::{
    create_establishment_with_member, delete_media_asset_for_est, find_establishment_by_id,
    find_establishment_for_account, find_media_asset_for_est, insert_media_asset,
    set_establishment_logo, slug_exists, update_establishment_profile,
};

pub struct CreateEstablishmentInput<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    pub accent_color: &'a str,
}

pub struct UpdateEstablishmentInput<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    pub description_en: &'a str,
    pub description_es: &'a str,
    pub accent_color: &'a str,
}

pub async fn create_establishment(
    pool: &SqlitePool,
    account_id: &str,
    input: CreateEstablishmentInput<'_>,
) -> AppResult<Establishment> {
    if let Some(existing) = find_establishment_for_account(pool, account_id).await? {
        return Ok(existing);
    }

    let name = validate_non_empty_name(input.name, 100)?;
    let slug_source = if input.slug.trim().is_empty() {
        name.as_str()
    } else {
        input.slug
    };
    let slug = Slug::from_input(slug_source)?;
    let color = AccentColor::parse(if input.accent_color.trim().is_empty() {
        AccentColor::DEFAULT_HEX
    } else {
        input.accent_color
    })?;
    let description = sanitize_optional_text(input.description, 280);

    if slug_exists(pool, slug.as_str(), None).await? {
        return Err(AppError::Conflict("error.slug_taken"));
    }

    let now = Utc::now().to_rfc3339();
    let est = Establishment {
        id: Uuid::now_v7().to_string(),
        slug: slug.as_str().to_string(),
        name,
        description,
        description_en: String::new(),
        description_es: String::new(),
        accent_color: color.as_str().to_string(),
        logo_asset_id: None,
        is_published: false,
        published_version: 0,
        has_unpublished_changes: true,
        last_published_at: None,
        created_at: now.clone(),
        updated_at: now,
    };

    if let Err(err) = create_establishment_with_member(pool, &est, account_id).await {
        if is_unique_violation(&err) {
            return Err(AppError::Conflict("error.slug_taken"));
        }
        return Err(err);
    }

    Ok(est)
}

pub async fn update_profile(
    pool: &SqlitePool,
    est: &Establishment,
    input: UpdateEstablishmentInput<'_>,
) -> AppResult<Establishment> {
    let name = validate_non_empty_name(input.name, 100)?;
    let color = AccentColor::parse(input.accent_color)?;
    let description = sanitize_optional_text(input.description, 280);
    let description_en = sanitize_optional_text(input.description_en, 280);
    let description_es = sanitize_optional_text(input.description_es, 280);

    let new_slug = if est.published_version > 0 {
        let requested = Slug::from_input(input.slug)?;
        if requested.as_str() != est.slug {
            return Err(AppError::Validation("error.slug_locked"));
        }
        est.slug.clone()
    } else {
        let requested = Slug::from_input(input.slug)?;
        if requested.as_str() != est.slug
            && slug_exists(pool, requested.as_str(), Some(&est.id)).await?
        {
            return Err(AppError::Conflict("error.slug_taken"));
        }
        requested.as_str().to_string()
    };

    let now = Utc::now().to_rfc3339();
    update_establishment_profile(
        pool,
        &est.id,
        &new_slug,
        &name,
        &description,
        &description_en,
        &description_es,
        color.as_str(),
        &now,
    )
    .await?;

    find_establishment_by_id(pool, &est.id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn upload_logo(
    pool: &SqlitePool,
    assets: &AssetProcessor,
    est: &Establishment,
    raw_bytes: Vec<u8>,
) -> AppResult<MediaAsset> {
    let old_asset = match &est.logo_asset_id {
        Some(id) => find_media_asset_for_est(pool, &est.id, id).await?,
        None => None,
    };

    let (asset, _, _) = assets
        .process_and_store(&est.id, MediaKind::Logo, raw_bytes)
        .await?;

    insert_media_asset(pool, &asset).await?;
    let now = Utc::now().to_rfc3339();
    set_establishment_logo(pool, &est.id, Some(&asset.id), &now).await?;

    if let Some(prev) = old_asset {
        let _ = delete_media_asset_for_est(pool, &est.id, &prev.id).await;
        assets.remove_files(&prev);
    }

    Ok(asset)
}

pub async fn remove_logo(
    pool: &SqlitePool,
    assets: &AssetProcessor,
    est: &Establishment,
) -> AppResult<()> {
    let old_asset = match &est.logo_asset_id {
        Some(id) => find_media_asset_for_est(pool, &est.id, id).await?,
        None => None,
    };
    let now = Utc::now().to_rfc3339();
    set_establishment_logo(pool, &est.id, None, &now).await?;

    if let Some(prev) = old_asset {
        let _ = delete_media_asset_for_est(pool, &est.id, &prev.id).await;
        assets.remove_files(&prev);
    }
    Ok(())
}
