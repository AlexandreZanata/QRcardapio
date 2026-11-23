use sqlx::SqlitePool;

use super::mappers::row_to_media;
use crate::domain::publication::MediaAsset;
use crate::error::AppResult;

pub async fn insert_media_asset(pool: &SqlitePool, asset: &MediaAsset) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO media_assets (
            id, establishment_id, kind, content_hash, mime_type,
            width, height, byte_size, filename_large, filename_thumb, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&asset.id)
    .bind(&asset.establishment_id)
    .bind(asset.kind.as_str())
    .bind(&asset.content_hash)
    .bind(&asset.mime_type)
    .bind(asset.width)
    .bind(asset.height)
    .bind(asset.byte_size)
    .bind(&asset.filename_large)
    .bind(&asset.filename_thumb)
    .bind(&asset.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_media_asset_for_est(
    pool: &SqlitePool,
    est_id: &str,
    asset_id: &str,
) -> AppResult<Option<MediaAsset>> {
    let row = sqlx::query("SELECT * FROM media_assets WHERE id = ? AND establishment_id = ?")
        .bind(asset_id)
        .bind(est_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_media(&r)))
}

pub async fn delete_media_asset_for_est(
    pool: &SqlitePool,
    est_id: &str,
    asset_id: &str,
) -> AppResult<()> {
    sqlx::query("DELETE FROM media_assets WHERE id = ? AND establishment_id = ?")
        .bind(asset_id)
        .bind(est_id)
        .execute(pool)
        .await?;
    Ok(())
}
