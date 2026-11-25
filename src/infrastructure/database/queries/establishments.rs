use sqlx::{Row, SqliteConnection, SqliteExecutor, SqlitePool};

use super::mappers::row_to_establishment;
use crate::domain::establishment::Establishment;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn is_establishment_owner(
    pool: &SqlitePool,
    est: &str,
    account: &str,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM establishment_members
         WHERE account_id = ? AND establishment_id = ? AND member_role = 'owner')",
    )
    .bind(account)
    .bind(est)
    .fetch_one(pool)
    .await?)
}

pub async fn find_establishment_for_account<'e>(
    executor: impl SqliteExecutor<'e>,
    account_id: &str,
) -> AppResult<Option<Establishment>> {
    let row = sqlx::query(
        "SELECT e.*
         FROM establishments e
         INNER JOIN establishment_members m ON m.establishment_id = e.id
         WHERE m.account_id = ?
         ORDER BY m.created_at ASC
         LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(executor)
    .await?;

    Ok(row.map(|r| row_to_establishment(&r)))
}

/// First establishment of an account together with the member role it holds there.
pub async fn find_membership_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> AppResult<Option<(Establishment, String)>> {
    let row = sqlx::query(
        "SELECT e.*, m.member_role AS membership_role
         FROM establishments e
         INNER JOIN establishment_members m ON m.establishment_id = e.id
         WHERE m.account_id = ?
         ORDER BY m.created_at ASC
         LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (row_to_establishment(&r), r.get("membership_role"))))
}

pub async fn find_establishment_by_id(
    pool: &SqlitePool,
    est_id: &str,
) -> AppResult<Option<Establishment>> {
    let row = sqlx::query("SELECT * FROM establishments WHERE id = ?")
        .bind(est_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_establishment(&r)))
}

pub async fn establishment_slug_exists(pool: &SqlitePool, slug: &str) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM establishments WHERE slug = ?)")
            .bind(slug)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn create_establishment_with_member(
    pool: &SqlitePool,
    mut est: Establishment,
    account_id: &str,
    alternatives: impl Iterator<Item = String> + Send,
) -> AppResult<Establishment> {
    let mut tx = begin_immediate(pool).await?;
    // Check inside the write transaction so parallel onboarding from one account is idempotent.
    if let Some(existing) = find_establishment_for_account(&mut *tx, account_id).await? {
        tx.commit().await?;
        return Ok(existing);
    }

    let mut candidates = std::iter::once(est.slug.clone()).chain(alternatives);
    loop {
        est.slug = candidates
            .next()
            .ok_or(AppError::Conflict("error.slug_taken"))?;
        if try_insert_establishment(&mut tx, &est).await? {
            break;
        }
    }

    sqlx::query(
        "INSERT INTO establishment_members (establishment_id, account_id, member_role, created_at)
         VALUES (?, ?, 'owner', ?)",
    )
    .bind(&est.id)
    .bind(account_id)
    .bind(&est.created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(est)
}

async fn try_insert_establishment(
    conn: &mut SqliteConnection,
    est: &Establishment,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT INTO establishments (
            id, slug, name, description, description_en, description_es,
            accent_color, logo_asset_id, is_published, published_version,
            has_unpublished_changes, last_published_at, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(slug) DO NOTHING",
    )
    .bind(&est.id)
    .bind(&est.slug)
    .bind(&est.name)
    .bind(&est.description)
    .bind(&est.description_en)
    .bind(&est.description_es)
    .bind(&est.accent_color)
    .bind(&est.logo_asset_id)
    .bind(i64::from(est.is_published))
    .bind(est.published_version)
    .bind(i64::from(est.has_unpublished_changes))
    .bind(&est.last_published_at)
    .bind(&est.created_at)
    .bind(&est.updated_at)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_establishment_profile(
    pool: &SqlitePool,
    est_id: &str,
    slug: &str,
    name: &str,
    description: &str,
    description_en: &str,
    description_es: &str,
    accent_color: &str,
    google_maps_url: Option<Option<&str>>,
    google_review_url: Option<Option<&str>>,
    now: &str,
) -> AppResult<Option<Establishment>> {
    let row = sqlx::query(
        "UPDATE establishments
         SET slug = ?, name = ?, description = ?, description_en = ?, description_es = ?,
             accent_color = ?,
             google_maps_url = CASE WHEN ? THEN ? ELSE google_maps_url END,
             google_review_url = CASE WHEN ? THEN ? ELSE google_review_url END,
             has_unpublished_changes = 1, updated_at = ?
         WHERE id = ? AND (published_version = 0 OR slug = ?)
         RETURNING *",
    )
    .bind(slug)
    .bind(name)
    .bind(description)
    .bind(description_en)
    .bind(description_es)
    .bind(accent_color)
    .bind(google_maps_url.is_some())
    .bind(google_maps_url.flatten())
    .bind(google_review_url.is_some())
    .bind(google_review_url.flatten())
    .bind(now)
    .bind(est_id)
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| row_to_establishment(&row)))
}

pub async fn set_establishment_logo(
    pool: &SqlitePool,
    est_id: &str,
    asset_id: Option<&str>,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE establishments
         SET logo_asset_id = ?, has_unpublished_changes = 1, updated_at = ?
         WHERE id = ?",
    )
    .bind(asset_id)
    .bind(now)
    .bind(est_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn mark_menu_draft_changed(
    conn: &mut sqlx::SqliteConnection,
    est_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE establishments SET has_unpublished_changes = 1, updated_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(est_id)
    .execute(conn)
    .await?;
    Ok(())
}
