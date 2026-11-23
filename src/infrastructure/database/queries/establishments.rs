use sqlx::SqlitePool;

use super::mappers::row_to_establishment;
use crate::domain::establishment::Establishment;
use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

pub async fn find_establishment_for_account(
    pool: &SqlitePool,
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
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_establishment(&r)))
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

pub async fn slug_exists(
    pool: &SqlitePool,
    slug: &str,
    exclude_est_id: Option<&str>,
) -> AppResult<bool> {
    let found: Option<i64> = match exclude_est_id {
        Some(eid) => {
            sqlx::query_scalar("SELECT 1 FROM establishments WHERE slug = ? AND id != ? LIMIT 1")
                .bind(slug)
                .bind(eid)
                .fetch_optional(pool)
                .await?
        }
        None => {
            sqlx::query_scalar("SELECT 1 FROM establishments WHERE slug = ? LIMIT 1")
                .bind(slug)
                .fetch_optional(pool)
                .await?
        }
    };
    Ok(found.is_some())
}

pub async fn create_establishment_with_member(
    pool: &SqlitePool,
    est: &Establishment,
    account_id: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    sqlx::query(
        "INSERT INTO establishments (
            id, slug, name, description, description_en, description_es,
            accent_color, logo_asset_id, is_published, published_version,
            has_unpublished_changes, last_published_at, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&mut *tx)
    .await?;

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
    Ok(())
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
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE establishments
         SET slug = ?, name = ?, description = ?, description_en = ?, description_es = ?,
             accent_color = ?, has_unpublished_changes = 1, updated_at = ?
         WHERE id = ?",
    )
    .bind(slug)
    .bind(name)
    .bind(description)
    .bind(description_en)
    .bind(description_es)
    .bind(accent_color)
    .bind(now)
    .bind(est_id)
    .execute(pool)
    .await?;
    Ok(())
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
