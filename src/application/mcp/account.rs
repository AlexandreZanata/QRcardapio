use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::application::entitlements::{Enforcement, authorize, can_publish_menu};
use crate::application::publishing::{
    PublishLocks, publish_establishment, unpublish_establishment,
};
use crate::config::Config;
use crate::domain::entitlements::{MemberRole, Operation as GuardedOperation};
use crate::domain::establishment::{
    AccentColor, Slug, sanitize_optional_text, validate_non_empty_name,
};
use crate::domain::google_maps::validate_google_maps_link;
use crate::domain::mcp::is_url_token;
use crate::domain::mcp_account::{AccountSettingsInput, ValidatedAccountPatch};
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{
    find_account_by_id, find_establishment_by_id, is_slug_locked, mcp_account as queries,
    mcp_menu::Operation,
    mcp_oauth::{Grant, has_active_owner_write_grant},
};
use crate::infrastructure::security::sha256_hex;

pub async fn read_settings(
    pool: &SqlitePool,
    enforcement: Enforcement,
    grant: &Grant,
    origin: &str,
) -> AppResult<Value> {
    let account = find_account_by_id(pool, &grant.account_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let est = find_establishment_by_id(pool, &grant.establishment_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let counts = queries::menu_summary_counts(pool, &est.id).await?;
    let can_publish = can_publish_menu(pool, enforcement, &est.id).await?;
    let slug_locked = is_slug_locked(pool, &est.id).await?;
    let accent_contrast_ink = AccentColor::parse(&est.accent_color)
        .map(|c| c.contrast_ink())
        .unwrap_or("#FFFFFF");

    Ok(json!({
        "account": {
            "email": account.email,
            "role": account.role.as_str(),
            "preferred_locale": account.preferred_locale.code(),
        },
        "establishment": {
            "id": est.id,
            "name": est.name,
            "slug": est.slug,
            "slug_locked": slug_locked,
            "description": est.description,
            "description_en": est.description_en,
            "description_es": est.description_es,
            "accent_color": est.accent_color,
            "accent_contrast_ink": accent_contrast_ink,
            "google_maps_url": est.google_maps_url,
            "google_review_url": est.google_review_url,
            "has_logo": est.logo_asset_id.is_some(),
            "updated_at": est.updated_at,
        },
        "menu_summary": {
            "category_count": counts.category_count,
            "visible_category_count": counts.visible_category_count,
            "product_count": counts.product_count,
            "visible_product_count": counts.visible_product_count,
        },
        "publication": {
            "is_published": est.is_published,
            "published_version": est.published_version,
            "has_unpublished_changes": est.has_unpublished_changes,
            "last_published_at": est.last_published_at,
            "billing_enabled": enforcement != Enforcement::NotEnforced,
            "can_publish": can_publish,
            "public_url": format!("{origin}/c/{}", est.slug),
            "preview_url": format!("{origin}/dashboard/preview"),
            "dashboard_url": format!("{origin}/dashboard"),
            "settings_url": format!("{origin}/dashboard/settings"),
            "billing_url": format!("{origin}/dashboard/billing"),
            "qr_url": format!("{origin}/dashboard/qr"),
        }
    }))
}

pub async fn update_settings(
    pool: &SqlitePool,
    locks: &PublishLocks,
    enforcement: Enforcement,
    grant: &Grant,
    input: AccountSettingsInput,
    origin: &str,
) -> AppResult<Value> {
    if input.is_empty() || !is_url_token(&input.operation_key, 4, 96) {
        return Err(AppError::Validation("mcp.invalid_changes"));
    }
    DateTime::parse_from_rfc3339(&input.updated_at)
        .map_err(|_| AppError::Validation("mcp.invalid_changes"))?;

    let fingerprint = sha256_hex(
        &serde_json::to_string(&input).map_err(|_| AppError::Validation("mcp.invalid_changes"))?,
    );

    let name = input
        .name
        .as_deref()
        .map(|raw| validate_non_empty_name(raw, 100))
        .transpose()?;
    let slug = input
        .slug
        .as_deref()
        .map(|raw| Slug::from_input(raw).map(Slug::into_string))
        .transpose()?;
    let description = input
        .description
        .as_deref()
        .map(|raw| sanitize_optional_text(raw, 280));
    let description_en = input
        .description_en
        .as_deref()
        .map(|raw| sanitize_optional_text(raw, 280));
    let description_es = input
        .description_es
        .as_deref()
        .map(|raw| sanitize_optional_text(raw, 280));
    let accent_color = input
        .accent_color
        .as_deref()
        .map(|raw| AccentColor::parse(raw).map(|c| c.as_str().to_string()))
        .transpose()?;
    let google_maps_url = match input.google_maps_url {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) => Some(validate_google_maps_link(&raw)?.map(str::to_string)),
    };
    let google_review_url = match input.google_review_url {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) => Some(validate_google_maps_link(&raw)?.map(str::to_string)),
    };
    let preferred_locale = match input.preferred_locale.as_deref() {
        None => None,
        Some(raw) => {
            Some(Locale::parse(raw.trim()).ok_or(AppError::Validation("mcp.invalid_changes"))?)
        }
    };

    let patch = ValidatedAccountPatch {
        expected_updated_at: input.updated_at,
        name,
        slug,
        description,
        description_en,
        description_es,
        accent_color,
        google_maps_url,
        google_review_url,
        preferred_locale,
    };

    let now = Utc::now();
    let timestamp = now.to_rfc3339();
    let lock = locks.lock_for(&grant.establishment_id);
    let _guard = lock.lock().await;

    let outcome = queries::apply_settings(
        pool,
        grant,
        Operation {
            key: &input.operation_key,
            hash: &fingerprint,
            now: now.timestamp(),
            timestamp: &timestamp,
        },
        patch,
    )
    .await?;

    let mut state_view = read_settings(pool, enforcement, grant, origin).await?;
    state_view["applied"] = serde_json::to_value(outcome)
        .map_err(|_| AppError::Internal("could not serialize account outcome".into()))?;
    Ok(state_view)
}

pub async fn publish(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    enforcement: Enforcement,
    grant: &Grant,
) -> AppResult<Value> {
    let now = Utc::now().timestamp();
    if !has_active_owner_write_grant(pool, grant, now).await? {
        return Err(AppError::Forbidden("error.forbidden"));
    }
    // The menu.write OAuth grant is owner-only, which narrows the operation's role set.
    authorize(
        pool,
        enforcement,
        &grant.establishment_id,
        MemberRole::Owner,
        GuardedOperation::PublishMenu,
        crate::domain::billing::billing_now().date_naive(),
    )
    .await?
    .map_err(|denial| denial.into_error())?;
    let outcome = publish_establishment(pool, config, locks, &grant.establishment_id).await?;
    let est = find_establishment_by_id(pool, &grant.establishment_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let origin = config.public_base_url.trim_end_matches('/');

    Ok(json!({
        "published": true,
        "slug": est.slug,
        "version": outcome.version,
        "html_bytes": outcome.html_bytes,
        "asset_count": outcome.asset_count,
        "duration_ms": outcome.duration_ms,
        "updated_at": est.updated_at,
        "public_url": config.public_menu_url(&est.slug),
        "qr_url": format!("{origin}/dashboard/qr"),
        "dashboard_url": format!("{origin}/dashboard"),
    }))
}

pub async fn unpublish(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    grant: &Grant,
) -> AppResult<Value> {
    let now = Utc::now().timestamp();
    if !has_active_owner_write_grant(pool, grant, now).await? {
        return Err(AppError::Forbidden("error.forbidden"));
    }
    let est = find_establishment_by_id(pool, &grant.establishment_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    unpublish_establishment(pool, config, locks, &est).await?;
    let updated = find_establishment_by_id(pool, &grant.establishment_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let origin = config.public_base_url.trim_end_matches('/');

    Ok(json!({
        "published": false,
        "slug": updated.slug,
        "published_version": updated.published_version,
        "updated_at": updated.updated_at,
        "dashboard_url": format!("{origin}/dashboard"),
        "preview_url": format!("{origin}/dashboard/preview"),
    }))
}
