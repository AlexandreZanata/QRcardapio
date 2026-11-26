use serde_json::json;
use sqlx::{Sqlite, SqliteExecutor, SqlitePool, Transaction};
use uuid::Uuid;

use super::{catalog_snapshots, mappers::row_to_establishment};
use crate::domain::delivery::storefront::{
    StorefrontSettings, StorefrontStatus, ValidatedStorefrontSettings,
};
use crate::domain::establishment::Establishment;
use crate::domain::events::{EventType, NewEvent};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;

#[derive(sqlx::FromRow)]
struct StorefrontRow {
    establishment_id: String,
    status: String,
    published_version: i64,
    has_unpublished_changes: bool,
    last_published_at: Option<String>,
    accept_cash: bool,
    accept_card_on_delivery: bool,
    accept_pix_on_delivery: bool,
    pix_key_display: String,
    contact_whatsapp: String,
    pickup_instructions: String,
    default_prep_minutes: i64,
    updated_at: String,
}

impl TryFrom<StorefrontRow> for StorefrontSettings {
    type Error = AppError;

    fn try_from(row: StorefrontRow) -> AppResult<Self> {
        Ok(Self {
            establishment_id: row.establishment_id,
            status: StorefrontStatus::parse(&row.status)
                .ok_or_else(|| AppError::Internal("invalid storefront status".into()))?,
            published_version: row.published_version,
            has_unpublished_changes: row.has_unpublished_changes,
            last_published_at: row.last_published_at,
            accept_cash: row.accept_cash,
            accept_card_on_delivery: row.accept_card_on_delivery,
            accept_pix_on_delivery: row.accept_pix_on_delivery,
            pix_key_display: row.pix_key_display,
            contact_whatsapp: row.contact_whatsapp,
            pickup_instructions: row.pickup_instructions,
            default_prep_minutes: row.default_prep_minutes,
            updated_at: row.updated_at,
        })
    }
}

pub async fn get_or_default<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
) -> AppResult<StorefrontSettings> {
    let row: Option<StorefrontRow> = sqlx::query_as(
        "SELECT establishment_id, status, published_version, has_unpublished_changes,
                last_published_at, accept_cash, accept_card_on_delivery,
                accept_pix_on_delivery, pix_key_display, contact_whatsapp,
                pickup_instructions, default_prep_minutes, updated_at
         FROM storefronts WHERE establishment_id = ?",
    )
    .bind(est_id)
    .fetch_optional(executor)
    .await?;
    match row {
        Some(row) => StorefrontSettings::try_from(row),
        None => Ok(StorefrontSettings::default_for(est_id)),
    }
}

pub async fn upsert_settings(
    pool: &SqlitePool,
    est_id: &str,
    settings: &ValidatedStorefrontSettings,
    now: &str,
) -> AppResult<StorefrontSettings> {
    let row: StorefrontRow = sqlx::query_as(
        "INSERT INTO storefronts (
            establishment_id, accept_cash, accept_card_on_delivery, accept_pix_on_delivery,
            pix_key_display, contact_whatsapp, pickup_instructions, default_prep_minutes, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(establishment_id) DO UPDATE SET
            accept_cash = excluded.accept_cash,
            accept_card_on_delivery = excluded.accept_card_on_delivery,
            accept_pix_on_delivery = excluded.accept_pix_on_delivery,
            pix_key_display = excluded.pix_key_display,
            contact_whatsapp = excluded.contact_whatsapp,
            pickup_instructions = excluded.pickup_instructions,
            default_prep_minutes = excluded.default_prep_minutes,
            has_unpublished_changes = 1,
            updated_at = excluded.updated_at
         RETURNING establishment_id, status, published_version, has_unpublished_changes,
                   last_published_at, accept_cash, accept_card_on_delivery,
                   accept_pix_on_delivery, pix_key_display, contact_whatsapp,
                   pickup_instructions, default_prep_minutes, updated_at",
    )
    .bind(est_id)
    .bind(settings.accept_cash)
    .bind(settings.accept_card_on_delivery)
    .bind(settings.accept_pix_on_delivery)
    .bind(&settings.pix_key_display)
    .bind(&settings.contact_whatsapp)
    .bind(&settings.pickup_instructions)
    .bind(settings.default_prep_minutes)
    .bind(now)
    .fetch_one(pool)
    .await?;
    StorefrontSettings::try_from(row)
}

pub async fn mark_unpublished_changes(pool: &SqlitePool, est_id: &str, now: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE storefronts SET has_unpublished_changes = 1, updated_at = ?
         WHERE establishment_id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) struct Publication<'a> {
    pub storefront_version: i64,
    pub slug: &'a str,
    pub kind: &'a str,
    pub source_updated_at: &'a str,
    pub payload_json: &'a str,
    pub html_bytes: i64,
    pub duration_ms: i64,
    pub asset_count: i64,
    pub now: &'a str,
}

pub(crate) async fn record_publication_success(
    pool: &SqlitePool,
    snapshot: &catalog_snapshots::CatalogSnapshot,
    previous_catalog_version: Option<i64>,
    input: Publication<'_>,
) -> AppResult<()> {
    let est_id = &snapshot.establishment_id;
    let Publication {
        storefront_version: new_version,
        slug,
        kind,
        source_updated_at,
        payload_json,
        html_bytes,
        duration_ms,
        asset_count,
        now,
    } = input;
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let current_slug: String = sqlx::query_scalar("SELECT slug FROM establishments WHERE id = ?")
        .bind(est_id)
        .fetch_one(&mut *tx)
        .await?;
    if current_slug != slug {
        return Err(AppError::Conflict("error.slug_locked"));
    }
    catalog_snapshots::activate_version_in(&mut tx, snapshot, previous_catalog_version).await?;
    sqlx::query(
        "INSERT INTO storefront_presentations(establishment_id, publication_version, payload_json)
         VALUES (?, ?, ?) ON CONFLICT(establishment_id) DO UPDATE SET
           publication_version = excluded.publication_version, payload_json = excluded.payload_json",
    )
    .bind(est_id)
    .bind(new_version)
    .bind(payload_json)
    .execute(&mut *tx)
    .await?;
    let catalog_bytes = i64::try_from(payload_json.len()).unwrap_or(i64::MAX);
    sqlx::query(
        "INSERT INTO storefronts (
            establishment_id, status, published_version, has_unpublished_changes,
            last_published_at, updated_at
         ) VALUES (?, 'published', ?, 0, ?, ?)
         ON CONFLICT(establishment_id) DO UPDATE SET
            status = 'published',
            published_version = excluded.published_version,
            has_unpublished_changes = CASE WHEN storefronts.updated_at = ? THEN 0 ELSE 1 END,
            last_published_at = excluded.last_published_at,
            updated_at = excluded.updated_at",
    )
    .bind(est_id)
    .bind(new_version)
    .bind(now)
    .bind(now)
    .bind(source_updated_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO storefront_publications (
            id, establishment_id, version, kind, status, html_bytes, catalog_bytes,
            duration_ms, error_message, published_at
         ) VALUES (?, ?, ?, ?, 'published', ?, ?, ?, NULL, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(est_id)
    .bind(new_version)
    .bind(kind)
    .bind(html_bytes)
    .bind(catalog_bytes)
    .bind(duration_ms)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let event = NewEvent::new(
        EventType::CatalogPublished,
        est_id,
        est_id,
        json!({
            "channel": "storefront",
            "publication_version": new_version,
            "html_bytes": html_bytes,
            "asset_count": asset_count,
        }),
    )
    .deduplicated(format!(
        "CatalogPublished:storefront:{est_id}:{new_version}"
    ));
    super::domain_events::append(&mut tx, &event).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn record_publication_failure(
    pool: &SqlitePool,
    est_id: &str,
    attempted_version: i64,
    kind: &str,
    error_message: &str,
    duration_ms: i64,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO storefront_publications (
            id, establishment_id, version, kind, status, html_bytes, catalog_bytes,
            duration_ms, error_message, published_at
         ) VALUES (?, ?, ?, ?, 'failed', 0, 0, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(est_id)
    .bind(attempted_version)
    .bind(kind)
    .bind(duration_ms)
    .bind(error_message)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn record_unpublish_in(
    tx: &mut Transaction<'_, Sqlite>,
    est_id: &str,
    current_version: i64,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE storefronts SET status = 'unpublished', has_unpublished_changes = 1, updated_at = ?
         WHERE establishment_id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(&mut **tx)
    .await?;
    if current_version >= 1 {
        sqlx::query(
            "INSERT INTO storefront_publications (
                id, establishment_id, version, kind, status, html_bytes, catalog_bytes,
                duration_ms, error_message, published_at
             ) VALUES (?, ?, ?, 'manual', 'unpublished', 0, 0, 0, NULL, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(est_id)
        .bind(current_version)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn find_published_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> AppResult<Option<(Establishment, StorefrontSettings)>> {
    let est_row = sqlx::query(
        "SELECT e.* FROM establishments e
         INNER JOIN storefronts s ON s.establishment_id = e.id
         WHERE e.slug = ? AND s.status = 'published'",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    let Some(est_row) = est_row else {
        return Ok(None);
    };
    let est = row_to_establishment(&est_row);
    let storefront = get_or_default(pool, &est.id).await?;
    Ok(Some((est, storefront)))
}

pub async fn find_establishment_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> AppResult<Option<Establishment>> {
    let row = sqlx::query("SELECT * FROM establishments WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_establishment(&r)))
}

pub(crate) async fn published_presentation<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT p.payload_json FROM storefront_presentations p
         JOIN storefronts s ON s.establishment_id = p.establishment_id
            AND s.published_version = p.publication_version
         WHERE p.establishment_id = ? AND s.status = 'published'",
    )
    .bind(est_id)
    .fetch_optional(executor)
    .await?)
}

pub(crate) async fn published_ids(pool: &SqlitePool) -> AppResult<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT establishment_id FROM storefronts WHERE status = 'published'")
            .fetch_all(pool)
            .await?,
    )
}
