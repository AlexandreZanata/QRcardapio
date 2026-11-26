use sqlx::{SqliteConnection, SqlitePool};

use super::{menu_options, publications};
use crate::domain::commerce::catalog_snapshot::{AvailabilityTarget, CatalogContent};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CatalogSnapshot {
    pub establishment_id: String,
    pub channel: String,
    pub version: i64,
    pub content_hash: String,
    pub snapshot_json: String,
    pub published_at: String,
    pub is_current: bool,
}

impl CatalogSnapshot {
    pub fn decode(&self) -> AppResult<CatalogContent> {
        CatalogContent::decode(&self.snapshot_json, &self.content_hash)
    }
}

pub async fn current(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
) -> AppResult<Option<CatalogSnapshot>> {
    let mut conn = pool.acquire().await?;
    current_in(&mut conn, est_id, channel).await
}

pub async fn current_in(
    conn: &mut SqliteConnection,
    est_id: &str,
    channel: &str,
) -> AppResult<Option<CatalogSnapshot>> {
    Ok(sqlx::query_as(
        "SELECT establishment_id, channel, version, content_hash, snapshot_json, published_at, is_current
         FROM catalog_snapshots WHERE channel = ? AND establishment_id = ? AND is_current = 1",
    )
    .bind(channel)
    .bind(est_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn find_version(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
    version: i64,
) -> AppResult<Option<CatalogSnapshot>> {
    Ok(sqlx::query_as(
        "SELECT establishment_id, channel, version, content_hash, snapshot_json, published_at, is_current
         FROM catalog_snapshots WHERE version = ? AND establishment_id = ? AND channel = ?",
    )
    .bind(version)
    .bind(est_id)
    .bind(channel)
    .fetch_optional(pool)
    .await?)
}

/// Explicit internal publication point for an ordering channel. Its caller authorizes
/// publication before calling. No preview, QR Menu republish, or checkout invokes it.
pub async fn publish(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
    now: &str,
) -> AppResult<CatalogSnapshot> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let mut menu = publications::load_menu_snapshot_in(&mut tx, est_id, false)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    menu_options::attach_catalog_options(&mut tx, &mut menu).await?;
    let content = CatalogContent::build(menu);
    let snapshot = insert_version(&mut tx, est_id, channel, now, &content).await?;
    tx.commit().await?;
    Ok(snapshot)
}

/// Publish an availability adjustment for a product in the current catalog.
pub async fn publish_product_availability(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
    product_id: &str,
    is_available: bool,
    now: &str,
) -> AppResult<CatalogSnapshot> {
    publish_availability(
        pool,
        est_id,
        channel,
        AvailabilityTarget::Product(product_id),
        is_available,
        now,
    )
    .await
}

/// Derive an availability-only version for a channel that the caller has authorized.
pub async fn publish_availability(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
    target: AvailabilityTarget<'_>,
    is_available: bool,
    now: &str,
) -> AppResult<CatalogSnapshot> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let previous = current_in(&mut tx, est_id, channel)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let mut content = previous.decode()?;
    if !content.set_availability(target, is_available)? {
        return Ok(previous);
    }
    let snapshot = insert_version(&mut tx, est_id, channel, now, &content).await?;
    tx.commit().await?;
    Ok(snapshot)
}

async fn insert_version(
    conn: &mut SqliteConnection,
    est_id: &str,
    channel: &str,
    now: &str,
    content: &CatalogContent,
) -> AppResult<CatalogSnapshot> {
    let last_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) FROM catalog_snapshots WHERE channel = ? AND establishment_id = ?",
    )
    .bind(channel)
    .bind(est_id)
    .fetch_one(&mut *conn)
    .await?;
    let version = last_version
        .checked_add(1)
        .ok_or_else(|| AppError::Internal("catalog snapshot version exhausted".into()))?;
    let (snapshot_json, content_hash) = content.canonical()?;

    sqlx::query(
        "UPDATE catalog_snapshots SET is_current = 0 WHERE channel = ? AND establishment_id = ? AND is_current = 1",
    )
    .bind(channel)
    .bind(est_id)
    .execute(&mut *conn)
    .await?;
    Ok(sqlx::query_as(
        "INSERT INTO catalog_snapshots (establishment_id, channel, version, content_hash, snapshot_json,
                                        published_at, is_current)
         VALUES (?, ?, ?, ?, ?, ?, 1)
         RETURNING establishment_id, channel, version, content_hash, snapshot_json, published_at, is_current",
    )
    .bind(est_id)
    .bind(channel)
    .bind(version)
    .bind(content_hash)
    .bind(snapshot_json)
    .bind(now)
    .fetch_one(&mut *conn)
    .await?)
}
