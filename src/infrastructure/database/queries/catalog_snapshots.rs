use sqlx::{SqliteConnection, SqlitePool};

use super::{menu_options, publications};
use crate::domain::commerce::catalog_snapshot::{AvailabilityTarget, CatalogContent};
use crate::domain::publication::MenuSnapshot;
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
    let (_, content) = capture_in(&mut tx, est_id).await?;
    let snapshot = insert_version(&mut tx, est_id, channel, now, &content, true).await?;
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
    let snapshot = insert_version(&mut tx, est_id, channel, now, &content, true).await?;
    tx.commit().await?;
    Ok(snapshot)
}

pub(crate) async fn capture_in(
    conn: &mut SqliteConnection,
    est_id: &str,
) -> AppResult<(MenuSnapshot, CatalogContent)> {
    let mut menu = publications::load_menu_snapshot_in(conn, est_id, false)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    menu_options::attach_catalog_options(conn, &mut menu).await?;
    let content = CatalogContent::build(menu.clone());
    Ok((menu, content))
}

/// Reserve a version while the publisher stages files. Checkouts keep using the current row.
pub(crate) async fn reserve_version(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
    now: &str,
    content: &CatalogContent,
) -> AppResult<CatalogSnapshot> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let snapshot = insert_version(&mut tx, est_id, channel, now, content, false).await?;
    tx.commit().await?;
    Ok(snapshot)
}

pub(crate) async fn activate_version_in(
    conn: &mut SqliteConnection,
    snapshot: &CatalogSnapshot,
    previous_version: Option<i64>,
) -> AppResult<()> {
    let current_version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM catalog_snapshots WHERE channel = ? AND establishment_id = ? AND is_current = 1",
    )
    .bind(&snapshot.channel)
    .bind(&snapshot.establishment_id)
    .fetch_optional(&mut *conn)
    .await?;
    if current_version != previous_version {
        return Err(AppError::Conflict("commerce.checkout_repriced"));
    }
    clear_current(conn, &snapshot.establishment_id, &snapshot.channel).await?;
    let active = sqlx::query(
        "UPDATE catalog_snapshots SET is_current = 1 WHERE channel = ? AND establishment_id = ?
             AND version = ? AND content_hash = ?",
    )
    .bind(&snapshot.channel)
    .bind(&snapshot.establishment_id)
    .bind(snapshot.version)
    .bind(&snapshot.content_hash)
    .execute(&mut *conn)
    .await?;
    if active.rows_affected() != 1 {
        return Err(AppError::Internal(
            "prepared catalog snapshot missing".into(),
        ));
    }
    Ok(())
}

async fn insert_version(
    conn: &mut SqliteConnection,
    est_id: &str,
    channel: &str,
    now: &str,
    content: &CatalogContent,
    is_current: bool,
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

    if is_current {
        clear_current(conn, est_id, channel).await?;
    }
    Ok(sqlx::query_as(
        "INSERT INTO catalog_snapshots (establishment_id, channel, version, content_hash, snapshot_json,
                                        published_at, is_current)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING establishment_id, channel, version, content_hash, snapshot_json, published_at, is_current",
    )
    .bind(est_id)
    .bind(channel)
    .bind(version)
    .bind(content_hash)
    .bind(snapshot_json)
    .bind(now)
    .bind(is_current)
    .fetch_one(&mut *conn)
    .await?)
}

async fn clear_current(conn: &mut SqliteConnection, est_id: &str, channel: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE catalog_snapshots SET is_current = 0 WHERE channel = ? AND establishment_id = ? AND is_current = 1",
    )
    .bind(channel)
    .bind(est_id)
    .execute(conn)
    .await?;
    Ok(())
}
