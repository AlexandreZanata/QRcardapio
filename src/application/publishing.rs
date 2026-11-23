use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;
use sqlx::SqlitePool;
use tokio::sync::Mutex as AsyncMutex;

use crate::config::Config;
use crate::domain::establishment::{Establishment, Slug};
use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{
    find_establishment_by_id, list_published_establishment_ids, load_menu_snapshot,
    record_publication_failure, record_publication_success, record_unpublish,
};
use crate::infrastructure::storage::{atomic_write_file, resolve_safe_subpath};
use crate::rendering::public_menu::{generate_brand_theme_css, render_public_menu_html};

#[derive(Clone, Default)]
pub struct PublishLocks {
    inner: Arc<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

impl PublishLocks {
    pub fn lock_for(&self, est_id: &str) -> Arc<AsyncMutex<()>> {
        let mut guard = self.inner.lock().expect("publish lock map poisoned");
        guard
            .entry(est_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }
}

#[derive(Debug, Clone)]
pub struct PublishOutcome {
    pub version: i64,
    pub html_bytes: usize,
    pub asset_count: usize,
    pub duration_ms: i64,
}

/// Publishes an establishment's visible menu into a self-contained static directory:
/// `published/{slug}/index.html`, `theme-{hash8}.css`, and content-addressed `media/*`.
/// Preserves any previous live snapshot if an error happens before the atomic `index.html` rename.
pub async fn publish_establishment(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    est_id: &str,
) -> AppResult<PublishOutcome> {
    let lock = locks.lock_for(est_id);
    let _guard = lock.lock().await;
    let started = Instant::now();

    let snapshot = load_menu_snapshot(pool, est_id, false)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let attempted_version = snapshot.establishment.published_version + 1;
    let res = write_static_snapshot(config, &snapshot).await;

    let elapsed_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let now = Utc::now().to_rfc3339();

    match res {
        Ok((html_bytes, asset_count)) => {
            record_publication_success(
                pool,
                est_id,
                attempted_version,
                html_bytes as i64,
                asset_count as i64,
                elapsed_ms,
                &now,
            )
            .await?;
            Ok(PublishOutcome {
                version: attempted_version,
                html_bytes,
                asset_count,
                duration_ms: elapsed_ms,
            })
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
    let slug = Slug::validate_existing(&snapshot.establishment.slug)?;
    let slug_str = slug.as_str();

    let pub_dir = resolve_safe_subpath(&config.published_dir(), slug_str)?;
    let pub_media_dir = pub_dir.join("media");
    let tmp_dir = config.published_tmp_dir();

    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::create_dir_all(&pub_media_dir)?;

    let (theme_filename, theme_css_bytes) =
        generate_brand_theme_css(&snapshot.establishment.accent_color);
    let theme_href = format!("/c/{slug_str}/{theme_filename}");
    let media_base = format!("/c/{slug_str}/media");

    let html = render_public_menu_html(snapshot, Locale::PtBr, false, &media_base, &theme_href)?;

    // Collect referenced content-addressed filenames.
    let mut keep_media: HashSet<String> = HashSet::new();
    if let Some(logo) = &snapshot.logo {
        keep_media.insert(logo.filename_thumb.clone());
    }
    for sc in &snapshot.categories {
        for sp in &sc.products {
            if let Some(img) = &sp.image {
                keep_media.insert(img.filename_thumb.clone());
            }
        }
    }

    // Stage all media assets first so the HTML never points to missing files.
    let src_media_dir = resolve_safe_subpath(&config.media_dir(), &snapshot.establishment.id)?;
    for filename in &keep_media {
        let dest = resolve_safe_subpath(&pub_media_dir, filename)?;
        if !dest.exists() {
            let src = resolve_safe_subpath(&src_media_dir, filename)?;
            let bytes = std::fs::read(&src)?;
            atomic_write_file(&tmp_dir, &dest, &bytes)?;
        }
    }

    // Stage fingerprinted theme stylesheet.
    let theme_dest = resolve_safe_subpath(&pub_dir, &theme_filename)?;
    if !theme_dest.exists() {
        atomic_write_file(&tmp_dir, &theme_dest, &theme_css_bytes)?;
    }

    // Atomic POSIX rename replaces index.html in a single filesystem step.
    let html_bytes = html.as_bytes();
    let index_dest = pub_dir.join("index.html");
    atomic_write_file(&tmp_dir, &index_dest, html_bytes)?;

    // Clean up obsolete theme files and unreferenced media only after index.html is live.
    cleanup_stale_files(&pub_dir, &theme_filename, &pub_media_dir, &keep_media);

    Ok((html_bytes.len(), keep_media.len()))
}

pub async fn unpublish_establishment(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
    est: &Establishment,
) -> AppResult<()> {
    let lock = locks.lock_for(&est.id);
    let _guard = lock.lock().await;

    let slug = Slug::validate_existing(&est.slug)?;
    let pub_dir = resolve_safe_subpath(&config.published_dir(), slug.as_str())?;
    if pub_dir.exists() {
        std::fs::remove_dir_all(&pub_dir)?;
    }

    let now = Utc::now().to_rfc3339();
    record_unpublish(pool, &est.id, est.published_version, &now).await?;
    Ok(())
}

pub async fn republish_all(
    pool: &SqlitePool,
    config: &Config,
    locks: &PublishLocks,
) -> AppResult<usize> {
    let ids = list_published_establishment_ids(pool).await?;
    let mut count = 0usize;
    for id in ids {
        if find_establishment_by_id(pool, &id).await?.is_some() {
            publish_establishment(pool, config, locks, &id).await?;
            count += 1;
        }
    }
    Ok(count)
}

fn cleanup_stale_files(
    pub_dir: &Path,
    active_theme: &str,
    pub_media_dir: &Path,
    keep_media: &HashSet<String>,
) {
    if let Ok(entries) = std::fs::read_dir(pub_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("theme-") && name.ends_with(".css") && name != active_theme {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(pub_media_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !keep_media.contains(&name) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
