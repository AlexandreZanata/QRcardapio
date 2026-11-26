use sqlx::SqlitePool;

use super::mcp_menu::Operation;
use super::mcp_oauth::Grant;
use crate::application::auth::is_unique_violation;
use crate::domain::mcp_account::{AccountPatchOutcome, ValidatedAccountPatch};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

#[derive(Debug, sqlx::FromRow)]
pub struct MenuSummaryCounts {
    pub category_count: i64,
    pub visible_category_count: i64,
    pub product_count: i64,
    pub visible_product_count: i64,
}

pub async fn menu_summary_counts(
    pool: &SqlitePool,
    establishment_id: &str,
) -> AppResult<MenuSummaryCounts> {
    let counts = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM categories WHERE establishment_id = ?) AS category_count,
            (SELECT COUNT(*) FROM categories WHERE establishment_id = ? AND is_visible = 1) AS visible_category_count,
            (SELECT COUNT(*) FROM products WHERE establishment_id = ?) AS product_count,
            (SELECT COUNT(*) FROM products WHERE establishment_id = ? AND is_visible = 1) AS visible_product_count",
    )
    .bind(establishment_id)
    .bind(establishment_id)
    .bind(establishment_id)
    .bind(establishment_id)
    .fetch_one(pool)
    .await?;
    Ok(counts)
}

pub async fn apply_settings(
    pool: &SqlitePool,
    grant: &Grant,
    operation: Operation<'_>,
    patch: ValidatedAccountPatch,
) -> AppResult<AccountPatchOutcome> {
    let mut tx = begin_immediate(pool).await?;
    let est = &grant.establishment_id;

    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mcp_grants g
            JOIN establishment_members m
              ON m.establishment_id = g.establishment_id AND m.account_id = g.account_id
         WHERE g.id = ? AND g.account_id = ? AND g.establishment_id = ? AND g.access_expires_at > ?
           AND (' ' || g.scope || ' ') LIKE '% menu.write %' AND m.member_role = 'owner')",
    )
    .bind(&grant.id)
    .bind(&grant.account_id)
    .bind(est)
    .bind(operation.now)
    .fetch_one(&mut *tx)
    .await?;
    if !valid {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    let previous: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT input_hash, result_json, expires_at FROM mcp_menu_operations
         WHERE operation_key = ? AND account_id = ? AND establishment_id = ?",
    )
    .bind(operation.key)
    .bind(&grant.account_id)
    .bind(est)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((hash, data, expires_at)) = previous
        && expires_at > operation.now
    {
        if hash != operation.hash {
            return Err(AppError::Conflict("mcp.operation_conflict"));
        }
        return serde_json::from_str(&data)
            .map_err(|_| AppError::Internal("invalid stored MCP result".into()));
    }

    let current: Option<(String, i64, i64, String)> = sqlx::query_as(
        "SELECT e.slug, e.published_version,
                COALESCE((SELECT s.published_version FROM storefronts s WHERE s.establishment_id = e.id), 0) AS storefront_version,
                e.updated_at
         FROM establishments e WHERE e.id = ?",
    )
    .bind(est)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((current_slug, published_version, storefront_version, current_updated_at)) = current
    else {
        return Err(AppError::NotFound("error.not_found"));
    };
    if current_updated_at != patch.expected_updated_at {
        return Err(AppError::Conflict("mcp.account_changed"));
    }
    if let Some(new_slug) = patch.slug.as_deref()
        && (published_version > 0 || storefront_version > 0)
        && new_slug != current_slug
    {
        return Err(AppError::Validation("error.slug_locked"));
    }

    let mut final_slug = current_slug;
    let mut final_updated_at = current_updated_at;
    let establishment_updated = patch.has_establishment_changes();
    if establishment_updated {
        let next_slug = patch.slug.as_deref().unwrap_or(&final_slug);
        let maps_present = patch.google_maps_url.is_some();
        let maps_val = patch.google_maps_url.as_ref().and_then(|v| v.as_deref());
        let review_present = patch.google_review_url.is_some();
        let review_val = patch.google_review_url.as_ref().and_then(|v| v.as_deref());

        let updated = sqlx::query(
            "UPDATE establishments
             SET slug = ?,
                 name = COALESCE(?, name),
                 description = COALESCE(?, description),
                 description_en = COALESCE(?, description_en),
                 description_es = COALESCE(?, description_es),
                 accent_color = COALESCE(?, accent_color),
                 google_maps_url = CASE WHEN ? THEN ? ELSE google_maps_url END,
                 google_review_url = CASE WHEN ? THEN ? ELSE google_review_url END,
                 has_unpublished_changes = 1,
                 updated_at = ?
             WHERE id = ? AND updated_at = ? AND (
                 (published_version = 0 AND NOT EXISTS (
                     SELECT 1 FROM storefronts s WHERE s.establishment_id = establishments.id AND s.published_version > 0
                 ))
                 OR slug = ?
             )",
        )
        .bind(next_slug)
        .bind(patch.name.as_deref())
        .bind(patch.description.as_deref())
        .bind(patch.description_en.as_deref())
        .bind(patch.description_es.as_deref())
        .bind(patch.accent_color.as_deref())
        .bind(maps_present)
        .bind(maps_val)
        .bind(review_present)
        .bind(review_val)
        .bind(operation.timestamp)
        .bind(est)
        .bind(&patch.expected_updated_at)
        .bind(next_slug)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from);

        match updated {
            Ok(res) if res.rows_affected() == 1 => {
                final_slug = next_slug.to_string();
                final_updated_at = operation.timestamp.to_string();
            }
            Ok(_) => return Err(AppError::Conflict("mcp.account_changed")),
            Err(err) if is_unique_violation(&err) => {
                return Err(AppError::Conflict("error.slug_taken"));
            }
            Err(err) => return Err(err),
        }
    }

    let mut final_locale = grant.preferred_locale.clone();
    let locale_updated = if let Some(locale) = patch.preferred_locale {
        sqlx::query("UPDATE accounts SET preferred_locale = ?, updated_at = ? WHERE id = ?")
            .bind(locale.code())
            .bind(operation.timestamp)
            .bind(&grant.account_id)
            .execute(&mut *tx)
            .await?;
        final_locale = locale.code().to_string();
        true
    } else {
        false
    };

    let outcome = AccountPatchOutcome {
        establishment_updated,
        locale_updated,
        slug: final_slug,
        updated_at: final_updated_at,
        preferred_locale: final_locale,
    };
    let result_json = serde_json::to_string(&outcome)
        .map_err(|_| AppError::Internal("could not encode MCP account result".into()))?;
    sqlx::query(
        "INSERT INTO mcp_menu_operations (establishment_id, account_id, operation_key, input_hash, result_json, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (establishment_id, account_id, operation_key) DO UPDATE SET input_hash = excluded.input_hash,
            result_json = excluded.result_json, expires_at = excluded.expires_at",
    )
    .bind(est)
    .bind(&grant.account_id)
    .bind(operation.key)
    .bind(operation.hash)
    .bind(result_json)
    .bind(operation.now + 7 * 86400)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(outcome)
}
