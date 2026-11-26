use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::RetiredAssets;
use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::storage::atomic_write_file;
use crate::rendering::public_menu::render_product_detail_html;

const DIRECTORY: &str = "p";

#[derive(Serialize, Deserialize)]
struct ProductAssetRefs {
    theme: String,
    image: Option<String>,
}

fn is_filename(name: &str) -> bool {
    !name.starts_with('.') && Path::new(name).file_name() == Some(OsStr::new(name))
}

struct ProductPage {
    filename: String,
    html: String,
    assets: ProductAssetRefs,
}

pub(super) struct ProductPages<'a> {
    pub(super) urls: HashMap<&'a str, String>,
    pages: Vec<ProductPage>,
}

impl<'a> ProductPages<'a> {
    pub(super) fn render(
        snapshot: &'a MenuSnapshot,
        media_base_url: &str,
        theme_href: &str,
        theme_filename: &str,
    ) -> AppResult<Self> {
        let mut urls = HashMap::new();
        let mut pages = Vec::new();
        for category in &snapshot.categories {
            for item in &category.products {
                let html = render_product_detail_html(
                    snapshot,
                    category,
                    item,
                    Locale::PtBr,
                    false,
                    media_base_url,
                    theme_href,
                )?;
                let mut hasher = Sha256::new();
                hasher.update(item.product.id.as_bytes());
                hasher.update([0]);
                hasher.update(html.as_bytes());
                let fingerprint = hex::encode(hasher.finalize());
                let filename = format!("{}.html", &fingerprint[..24]);
                urls.insert(
                    item.product.id.as_str(),
                    format!("/c/{}/{DIRECTORY}/{filename}", snapshot.establishment.slug),
                );
                pages.push(ProductPage {
                    filename,
                    html,
                    assets: ProductAssetRefs {
                        theme: theme_filename.to_owned(),
                        image: item
                            .image
                            .as_ref()
                            .map(|image| image.filename_large.clone()),
                    },
                });
            }
        }
        Ok(Self { urls, pages })
    }

    pub(super) fn stage(
        &self,
        pub_dir: &Path,
        tmp_dir: &Path,
        retired: &RetiredAssets,
    ) -> AppResult<()> {
        let page_dir = pub_dir.join(DIRECTORY);
        let refs_dir = retired.product_refs_dir();
        for page in &self.pages {
            let destination = page_dir.join(&page.filename);
            let refs = refs_dir.join(format!("{}.json", page.filename));
            retired.keep_live("products", &page.filename)?;
            if !refs.is_file() {
                let bytes = serde_json::to_vec(&page.assets).map_err(|e| {
                    AppError::Internal(format!("product assets serialization: {e}"))
                })?;
                atomic_write_file(tmp_dir, &refs, &bytes)?;
            }
            if !destination.is_file() {
                atomic_write_file(tmp_dir, &destination, page.html.as_bytes())?;
            }
        }
        Ok(())
    }

    /// Keep media and themes while an older index can still link to their product pages.
    /// Their own retirement window starts only after the last referring page is removed.
    pub(super) fn prune_and_retain_assets(
        &self,
        pub_dir: &Path,
        retired: &RetiredAssets,
        now: SystemTime,
        keep_media: &mut HashSet<String>,
        keep_themes: &mut HashSet<String>,
    ) -> AppResult<()> {
        let page_dir = pub_dir.join(DIRECTORY);
        let refs_dir = retired.product_refs_dir();
        let live: HashSet<_> = self.pages.iter().map(|p| p.filename.as_str()).collect();
        retired.prune(&page_dir, "products", now, |name| {
            name.ends_with(".html") && !live.contains(name)
        });
        if page_dir.is_dir() {
            for entry in fs::read_dir(&page_dir)? {
                let entry = entry?;
                let filename = entry.file_name();
                let Some(name) = filename.to_str() else {
                    continue;
                };
                if !name.ends_with(".html") || live.contains(name) {
                    continue;
                }
                let bytes = fs::read(refs_dir.join(format!("{name}.json")))?;
                let refs: ProductAssetRefs = serde_json::from_slice(&bytes)
                    .map_err(|e| AppError::Internal(format!("invalid product asset refs: {e}")))?;
                if !is_filename(&refs.theme)
                    || refs
                        .image
                        .as_deref()
                        .is_some_and(|image| !is_filename(image))
                {
                    return Err(AppError::Internal("invalid product asset filename".into()));
                }
                if keep_themes.insert(refs.theme.clone()) {
                    retired.keep_live("themes", &refs.theme)?;
                }
                if let Some(image) = refs.image
                    && keep_media.insert(image.clone())
                {
                    retired.keep_live("media", &image)?;
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&refs_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(page_name) = name.to_str().and_then(|name| name.strip_suffix(".json"))
                    && !page_dir.join(page_name).is_file()
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn retired_product_pages_hold_their_assets_until_the_page_is_pruned() {
        let root = tempfile::tempdir().unwrap();
        let published = root.path().join("published");
        let staging = root.path().join(".tmp");
        let retired = RetiredAssets::new(&staging, "test-menu");
        let first = ProductPages {
            urls: HashMap::new(),
            pages: vec![ProductPage {
                filename: "first.html".into(),
                html: "<!DOCTYPE html>".into(),
                assets: ProductAssetRefs {
                    theme: "theme-prior.css".into(),
                    image: Some("prior.jpg".into()),
                },
            }],
        };
        first.stage(&published, &staging, &retired).unwrap();
        let page_path = published.join("p/first.html");
        assert!(page_path.is_file());

        let next = ProductPages {
            urls: HashMap::new(),
            pages: vec![],
        };
        let retired_at = SystemTime::UNIX_EPOCH + Duration::from_secs(5_000);
        let mut media = HashSet::new();
        let mut themes = HashSet::new();
        next.prune_and_retain_assets(&published, &retired, retired_at, &mut media, &mut themes)
            .unwrap();
        assert!(page_path.is_file());
        assert!(media.contains("prior.jpg"));
        assert!(themes.contains("theme-prior.css"));

        media.clear();
        themes.clear();
        next.prune_and_retain_assets(
            &published,
            &retired,
            retired_at + Duration::from_secs(361),
            &mut media,
            &mut themes,
        )
        .unwrap();
        assert!(!page_path.exists());
        assert!(media.is_empty());
        assert!(themes.is_empty());
        assert!(!retired.product_refs_dir().join("first.html.json").exists());
    }
}
