use std::collections::HashMap;

use sqlx::{SqliteConnection, SqlitePool};

use super::{addon_groups, addon_options, product_variations, publications};
use crate::domain::commerce::catalog_snapshot::{CatalogAddonGroup, CatalogContent};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

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
    let mut tx = begin_immediate(pool).await?;
    let menu = publications::load_menu_snapshot_in(&mut tx, est_id, false)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let mut variations = HashMap::new();
    for (product_id, variation) in product_variations::list_in(&mut tx, est_id, None).await? {
        variations
            .entry(product_id)
            .or_insert_with(Vec::new)
            .push(variation);
    }
    let mut options = HashMap::new();
    for (group_id, option) in addon_options::list_in(&mut tx, est_id, None).await? {
        options
            .entry(group_id)
            .or_insert_with(Vec::new)
            .push(option);
    }
    let groups: HashMap<_, _> = addon_groups::list_in(&mut tx, est_id)
        .await?
        .into_iter()
        .map(|group| {
            let options = options.remove(&group.id).unwrap_or_default();
            (group.id.clone(), CatalogAddonGroup::new(&group, options))
        })
        .collect();
    let mut product_groups = HashMap::new();
    for (product_id, group_id) in addon_groups::list_product_links_in(&mut tx, est_id).await? {
        let group = groups
            .get(&group_id)
            .ok_or_else(|| AppError::Internal("catalog link references a missing group".into()))?;
        product_groups
            .entry(product_id)
            .or_insert_with(Vec::new)
            .push(group.clone());
    }

    let content = CatalogContent::build(menu, variations, product_groups);
    let snapshot = insert_version(&mut tx, est_id, channel, now, &content).await?;
    tx.commit().await?;
    Ok(snapshot)
}

/// Publish an availability adjustment from the current publicable catalog alone.
pub async fn publish_product_availability(
    pool: &SqlitePool,
    est_id: &str,
    channel: &str,
    product_id: &str,
    is_available: bool,
    now: &str,
) -> AppResult<CatalogSnapshot> {
    let mut tx = begin_immediate(pool).await?;
    let previous = current_in(&mut tx, est_id, channel)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let mut content = previous.decode()?;
    if content
        .product(product_id)
        .is_some_and(|product| product.is_available == is_available)
    {
        return Ok(previous);
    }
    content.set_product_availability(product_id, is_available)?;
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
