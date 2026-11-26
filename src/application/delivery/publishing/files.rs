use std::collections::HashSet;
use std::fs;
use std::io::{Cursor, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use image::imageops::{FilterType, overlay};
use image::{ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};
use tempfile::{TempDir, tempdir_in};

use crate::application::publishing::retired_assets::RetiredAssets;
use crate::config::Config;
use crate::domain::establishment::{AccentColor, Slug};
use crate::error::{AppError, AppResult};
use crate::infrastructure::storage::{atomic_write_file, resolve_safe_subpath};
use crate::rendering::assets::ASSET_VERSION;
use crate::rendering::public_menu::generate_brand_theme_css;
use crate::rendering::storefront::{StorefrontCatalogJson, render_storefront_html};

const STATIC_SHELL: &[&str] = &[
    "/static/css/tokens.css",
    "/static/css/language-picker.css",
    "/static/css/menu.css",
    "/static/css/storefront.css",
    "/static/js/language-picker.js",
    "/static/js/storefront.js",
    "/static/js/storefront-cart.js",
    "/static/js/storefront-checkout.js",
    "/static/js/storefront-tracking.js",
];

pub(super) struct PreparedFiles {
    stage: TempDir,
    active: PathBuf,
    backup: PathBuf,
    staging_root: PathBuf,
    slug: String,
    media: HashSet<String>,
    theme_name: String,
    pub html_bytes: usize,
}

impl PreparedFiles {
    pub fn prepare(
        config: &Config,
        payload: &StorefrontCatalogJson,
        payload_json: &str,
    ) -> AppResult<Self> {
        let slug = Slug::validate_existing(&payload.slug)?.as_str().to_owned();
        let active = resolve_safe_subpath(&config.storefronts_dir(), &slug)?;
        let uploads = resolve_safe_subpath(&config.media_dir(), &payload.establishment_id)?;
        // Backups of the media volume include the last committed public media even
        // after an owner deletes or replaces its draft copy.
        let backup = uploads.join("storefront-published");
        let staging_root = config.storefronts_tmp_dir();
        fs::create_dir_all(&staging_root)?;
        let stage = tempdir_in(&staging_root)?;
        let scope = format!("/s/{slug}/");
        let (theme_name, theme_bytes) = generate_brand_theme_css(&payload.accent_color);
        let theme_href = format!("{scope}{theme_name}");
        let html = render_storefront_html(payload, &theme_href, &config.public_base_url)?;
        let mut media = HashSet::new();
        for url in payload.logo_url.iter().chain(&payload.cover_url).chain(
            payload.categories.iter().flat_map(|category| {
                category
                    .products
                    .iter()
                    .filter_map(|product| product.image_url.as_ref())
            }),
        ) {
            media.insert(filename_for(&scope, url)?.to_owned());
        }
        for name in &media {
            let vault_file = resolve_safe_subpath(&backup, name)?;
            if !vault_file.is_file() {
                let source = resolve_safe_subpath(&uploads, name)?;
                let bytes = read_media_source(&source, &active.join("media").join(name))?;
                atomic_write_file(&backup, &vault_file, &bytes)?;
            }
        }

        let logo = payload
            .logo_url
            .as_deref()
            .map(|url| filename_for(&scope, url).and_then(|name| Ok(fs::read(backup.join(name))?)))
            .transpose()?;
        for size in [192, 512] {
            let bytes = build_icon(logo.as_deref(), &payload.accent_color, size)?;
            atomic_write_file(
                &staging_root,
                &stage.path().join(format!("icon-{size}.png")),
                &bytes,
            )?;
        }

        let version = payload.storefront_version;
        let catalog_name = format!("catalog.v{}.json", payload.catalog_version);
        let manifest = serde_json::json!({
            "id": scope, "start_url": scope, "scope": scope, "display": "standalone",
            "name": payload.name, "short_name": payload.name.chars().take(24).collect::<String>(),
            "theme_color": payload.accent_color, "background_color": "#faf7f2",
            "icons": [
                { "src": format!("icon-192.png?v={version}"), "sizes": "192x192", "type": "image/png" },
                { "src": format!("icon-512.png?v={version}"), "sizes": "512x512", "type": "image/png", "purpose": "any" }
            ]
        });
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(json_error)?;
        let mut shell: Vec<_> = STATIC_SHELL
            .iter()
            .map(|url| format!("{url}?v={ASSET_VERSION}"))
            .collect();
        shell.extend([
            scope.clone(),
            theme_href,
            format!("{scope}manifest.webmanifest"),
            format!("{scope}icon-192.png?v={version}"),
            format!("{scope}icon-512.png?v={version}"),
            format!("{scope}{catalog_name}"),
            "/static/icons/menu-cover.svg".into(),
            "/static/icons/menu-search.svg".into(),
            "/static/icons/language-pt.svg".into(),
            "/static/icons/language-en.svg".into(),
            "/static/icons/language-es.svg".into(),
        ]);
        shell.extend(payload.cover_url.iter().chain(&payload.logo_url).cloned());
        shell.sort_unstable();
        shell.dedup();
        let cache_prefix = format!("qrc-s-{}-", payload.establishment_id);
        let cache_name = format!("{cache_prefix}v{version}-{ASSET_VERSION}");
        let worker = format!(
            "const CACHE_PREFIX = {};\nconst CACHE_NAME = {};\nconst SCOPE = {};\nconst SHELL = {};\n{}",
            serde_json::to_string(&cache_prefix).map_err(json_error)?,
            serde_json::to_string(&cache_name).map_err(json_error)?,
            serde_json::to_string(&scope).map_err(json_error)?,
            serde_json::to_string(&shell).map_err(json_error)?,
            include_str!("worker.js")
        );
        for (name, bytes) in [
            ("index.html", html.as_bytes()),
            ("manifest.webmanifest", manifest_bytes.as_slice()),
            ("sw.js", worker.as_bytes()),
            (&catalog_name, payload_json.as_bytes()),
            (&theme_name, theme_bytes.as_slice()),
        ] {
            atomic_write_file(&staging_root, &stage.path().join(name), bytes)?;
        }

        // Prepare all public paths before the DB transaction. Failed writes in this
        // phase leave the previous index and active checkout snapshot untouched.
        fs::create_dir_all(active.join("media"))?;
        for entry in fs::read_dir(stage.path())? {
            let entry = entry?;
            if active.join(entry.file_name()).is_dir() {
                return Err(AppError::Internal(
                    "storefront output path is a directory".into(),
                ));
            }
        }
        for name in &media {
            let destination = resolve_safe_subpath(&active.join("media"), name)?;
            if !destination.is_file() {
                let bytes = fs::read(backup.join(name))?;
                atomic_write_file(&staging_root, &destination, &bytes)?;
            }
        }
        Ok(Self {
            stage,
            active,
            backup,
            staging_root,
            slug,
            media,
            theme_name,
            html_bytes: html.len(),
        })
    }

    pub fn asset_count(&self) -> usize {
        self.media.len()
    }

    /// The committed DB payload can reconstruct these files if the process stops
    /// before the final index swap. Readers see a complete old or new HTML file.
    pub fn install(self) -> AppResult<()> {
        for entry in fs::read_dir(self.stage.path())? {
            let entry = entry?;
            if entry.file_name() != "index.html" {
                fs::rename(entry.path(), self.active.join(entry.file_name()))?;
            }
        }
        fs::rename(
            self.stage.path().join("index.html"),
            self.active.join("index.html"),
        )?;
        let retired = RetiredAssets::new(&self.staging_root, &self.slug);
        for name in &self.media {
            retired.keep_live("media", name)?;
        }
        retired.keep_live("themes", &self.theme_name)?;
        let now = SystemTime::now();
        retired.prune(&self.active, "themes", now, |name| {
            name.starts_with("theme-") && name.ends_with(".css") && name != self.theme_name
        });
        retired.prune(&self.active.join("media"), "media", now, |name| {
            !self.media.contains(name)
        });
        // Keep current public assets available in the ordinary media-volume backup.
        retired.prune(&self.backup, "backup", now, |name| {
            !self.media.contains(name)
        });
        prune_catalogs(&self.active);
        Ok(())
    }
}

fn filename_for<'a>(scope: &str, url: &'a str) -> AppResult<&'a str> {
    url.strip_prefix(scope)
        .and_then(|rest| rest.strip_prefix("media/"))
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| AppError::Internal("invalid storefront media path".into()))
}

fn read_media_source(upload: &Path, live: &Path) -> AppResult<Vec<u8>> {
    match fs::read(upload) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(fs::read(live)?),
        result => Ok(result?),
    }
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::Internal(format!("storefront JSON: {error}"))
}

fn build_icon(logo: Option<&[u8]>, accent: &str, size: u32) -> AppResult<Vec<u8>> {
    let Some(logo) = logo.and_then(|bytes| image::load_from_memory(bytes).ok()) else {
        return Ok(match size {
            192 => include_bytes!("../../../../static/icons/icon-192.png").to_vec(),
            _ => include_bytes!("../../../../static/icons/icon-512.png").to_vec(),
        });
    };
    let accent = AccentColor::parse(accent)?;
    let (r, g, b) = accent.rgb();
    let mut pixels = RgbaImage::from_pixel(size, size, Rgba([r, g, b, 255]));
    let scaled = logo
        .resize(size * 3 / 4, size * 3 / 4, FilterType::Lanczos3)
        .to_rgba8();
    overlay(
        &mut pixels,
        &scaled,
        ((size - scaled.width()) / 2).into(),
        ((size - scaled.height()) / 2).into(),
    );
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out))
        .write_image(&pixels, size, size, image::ExtendedColorType::Rgba8)
        .map_err(|error| AppError::Internal(format!("storefront icon: {error}")))?;
    Ok(out)
}

fn prune_catalogs(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut versions: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let version = name
                .strip_prefix("catalog.v")?
                .strip_suffix(".json")?
                .parse::<i64>()
                .ok()?;
            Some((version, entry.path()))
        })
        .collect();
    versions.sort_unstable_by_key(|(version, _)| std::cmp::Reverse(*version));
    for (_, path) in versions.into_iter().skip(3) {
        let _ = fs::remove_file(path);
    }
}
