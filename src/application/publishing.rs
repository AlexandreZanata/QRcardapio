use std::borrow::Cow;
use std::collections::HashSet;
use std::time::{Instant, SystemTime};

use chrono::Utc;
use sqlx::SqlitePool;

use crate::config::Config;
use crate::domain::establishment::{Establishment, Slug};
use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{
    capture_menu_publication, find_establishment_by_id, list_published_establishment_ids,
    record_publication_failure, record_publication_success, record_unpublish,
};
use crate::infrastructure::storage::{atomic_write_file, resolve_safe_subpath};
use crate::rendering::public_menu::{generate_brand_theme_css, render_public_menu_html};

mod locks;
mod product_pages;
pub(in crate::application) mod retired_assets;
pub use locks::PublishLocks;
use product_pages::ProductPages;
use retired_assets::RetiredAssets;

#[derive(Debug, Clone)]
pub struct PublishOutcome {
    pub version: i64,
    pub html_bytes: usize,
    pub asset_count: usize,
    pub duration_ms: i64,
}

/// Publishes an establishment's visible menu into a self-contained static directory:
/// `published/{slug}/index.html`, product pages, a theme, and content-addressed media.
/// Preserves any previous live snapshot if an error happens before the atomic `index.html` rename.
pub async fn publish_establishment(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    est_id: &str,
) -> AppResult<PublishOutcome> {
    publish_menu(pool, config, locks, est_id, false)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

async fn publish_menu(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    est_id: &str,
    republishing: bool,
) -> AppResult<Option<PublishOutcome>> {
    let _guard = locks
        .lock_for_publication(&config.published_tmp_dir(), est_id)
        .await?;
    let started = Instant::now();

    let Some(source) = capture_menu_publication(pool, est_id).await? else {
        return Ok(None);
    };
    let snapshot = &source.snapshot;

    // The guard must use the same database snapshot as rendering the menu.
    if republishing
        && (!snapshot.establishment.is_published || snapshot.establishment.has_unpublished_changes)
    {
        tracing::info!(establishment_id = %est_id, "republish skipped: no matching live menu");
        return Ok(None);
    }

    let attempted_version = snapshot.establishment.published_version + 1;
    let res = write_static_snapshot(config, snapshot).await;

    let elapsed_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let now = Utc::now().to_rfc3339();

    match res {
        Ok((html_bytes, asset_count)) => {
            record_publication_success(
                pool,
                &source,
                attempted_version,
                html_bytes as i64,
                asset_count as i64,
                elapsed_ms,
                &now,
            )
            .await?;
            Ok(Some(PublishOutcome {
                version: attempted_version,
                html_bytes,
                asset_count,
                duration_ms: elapsed_ms,
            }))
        }
        Err(err) => {
            tracing::error!(
                establishment_id = %est_id,
                slug = %snapshot.establishment.slug,
                error = %err,
                "static menu publication failed; previous snapshot preserved"
            );
            let _ = record_publication_failure(
                pool,
                est_id,
                attempted_version,
                &err.to_string(),
                elapsed_ms,
                &now,
            )
            .await;
            Err(AppError::Internal(format!("publication failed: {err}")))
        }
    }
}

pub async fn write_static_snapshot(
    config: &Config,
    snapshot: &MenuSnapshot,
) -> AppResult<(usize, usize)> {
    let src_media_dir = resolve_safe_subpath(&config.media_dir(), &snapshot.establishment.id)?;
    write_static_snapshot_with_media(config, snapshot, |filename| {
        let src = resolve_safe_subpath(&src_media_dir, filename)?;
        Ok(Cow::Owned(std::fs::read(src)?))
    })
}

pub(crate) fn write_static_snapshot_with_media<'a>(
    config: &Config,
    snapshot: &MenuSnapshot,
    load_media: impl Fn(&str) -> AppResult<Cow<'a, [u8]>>,
) -> AppResult<(usize, usize)> {
    let slug = Slug::validate_existing(&snapshot.establishment.slug)?;
    let slug_str = slug.as_str();

    let pub_dir = resolve_safe_subpath(&config.published_dir(), slug_str)?;
    let pub_media_dir = pub_dir.join("media");
    let tmp_dir = config.published_tmp_dir();
    let retired = RetiredAssets::new(&tmp_dir, slug_str);

    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::create_dir_all(&pub_media_dir)?;

    let (theme_filename, theme_css_bytes) =
        generate_brand_theme_css(&snapshot.establishment.accent_color);
    let theme_href = format!("/c/{slug_str}/{theme_filename}");
    let media_base = format!("/c/{slug_str}/media");

    let product_pages = ProductPages::render(snapshot, &media_base, &theme_href, &theme_filename)?;
    let html = render_public_menu_html(
        snapshot,
        Locale::PtBr,
        false,
        &media_base,
        &theme_href,
        &config.public_base_url,
        &product_pages.urls,
    )?;

    // Collect referenced content-addressed filenames.
    let mut keep_media: HashSet<String> = HashSet::new();
    if let Some(logo) = &snapshot.logo {
        keep_media.insert(logo.filename_thumb.clone());
    }
    if let Some(cover) = snapshot.cover_image() {
        keep_media.insert(cover.filename_large.clone());
    }
    for sc in &snapshot.categories {
        for sp in &sc.products {
            if let Some(img) = &sp.image {
                keep_media.insert(img.filename_thumb.clone());
                keep_media.insert(img.filename_large.clone());
            }
        }
    }

    // Stage all media assets first so the HTML never points to missing files.
    for filename in &keep_media {
        let dest = resolve_safe_subpath(&pub_media_dir, filename)?;
        retired.keep_live("media", filename)?;
        if !dest.exists() {
            let bytes = load_media(filename)?;
            atomic_write_file(&tmp_dir, &dest, &bytes)?;
        }
    }

    // Stage fingerprinted theme stylesheet.
    let theme_dest = resolve_safe_subpath(&pub_dir, &theme_filename)?;
    retired.keep_live("themes", &theme_filename)?;
    if !theme_dest.exists() {
        atomic_write_file(&tmp_dir, &theme_dest, &theme_css_bytes)?;
    }

    product_pages.stage(&pub_dir, &tmp_dir, &retired)?;

    // Atomic POSIX rename replaces index.html in a single filesystem step.
    let html_bytes = html.as_bytes();
    let index_dest = pub_dir.join("index.html");
    atomic_write_file(&tmp_dir, &index_dest, html_bytes)?;

    // Cached HTML still references assets from the preceding publication.
    let now = SystemTime::now();
    let asset_count = keep_media.len();
    let mut keep_themes = HashSet::from([theme_filename]);
    match product_pages.prune_and_retain_assets(
        &pub_dir,
        &retired,
        now,
        &mut keep_media,
        &mut keep_themes,
    ) {
        Ok(()) => {
            retired.prune(&pub_dir, "themes", now, |name| {
                name.starts_with("theme-") && name.ends_with(".css") && !keep_themes.contains(name)
            });
            retired.prune(&pub_media_dir, "media", now, |name| {
                !keep_media.contains(name)
            });
        }
        Err(error) => {
            tracing::warn!(slug = slug_str, %error, "product page asset cleanup deferred");
        }
    }

    Ok((html_bytes.len(), asset_count))
}

pub async fn unpublish_establishment(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    est: &Establishment,
) -> AppResult<()> {
    let _guard = locks
        .lock_for_publication(&config.published_tmp_dir(), &est.id)
        .await?;
    let est = find_establishment_by_id(pool, &est.id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let slug = Slug::validate_existing(&est.slug)?;
    let pub_dir = resolve_safe_subpath(&config.published_dir(), slug.as_str())?;
    if pub_dir.exists() {
        std::fs::remove_dir_all(&pub_dir)?;
    }
    RetiredAssets::new(&config.published_tmp_dir(), slug.as_str()).clear();

    let now = Utc::now().to_rfc3339();
    record_unpublish(pool, &est.id, est.published_version, &now).await?;
    Ok(())
}

/// Re-renders live snapshots after a template or asset upgrade. Only establishments whose
/// draft matches the live menu are re-rendered; pending drafts are left for their owners.
pub async fn republish_all(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
) -> AppResult<usize> {
    let ids = list_published_establishment_ids(pool).await?;
    let mut count = 0usize;
    for id in ids {
        if publish_menu(pool, config, locks, &id, true)
            .await?
            .is_some()
        {
            count += 1;
        }
    }
    Ok(count)
}
