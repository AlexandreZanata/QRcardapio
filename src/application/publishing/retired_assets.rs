use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::infrastructure::storage::atomic_write_file;

// The HTML can remain usable for max-age=60 plus stale-while-revalidate=300.
const HTML_CACHE_WINDOW: Duration = Duration::from_secs(360);

pub(super) struct RetiredAssets {
    root: PathBuf,
}

impl RetiredAssets {
    pub(super) fn new(staging_dir: &Path, slug: &str) -> Self {
        Self {
            root: staging_dir.join("retired-assets").join(slug),
        }
    }

    pub(super) fn keep_live(&self, folder: &str, filename: &str) -> io::Result<()> {
        match fs::remove_file(self.root.join(folder).join(filename)) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    pub(super) fn prune(
        &self,
        dir: &Path,
        folder: &str,
        now: SystemTime,
        is_retired: impl Fn(&str) -> bool,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let Ok(epoch) = now.duration_since(SystemTime::UNIX_EPOCH) else {
            return;
        };
        let now_ms = epoch.as_millis();
        let markers = self.root.join(folder);

        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !is_retired(name) || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let marker = markers.join(name);
            match fs::read_to_string(&marker) {
                Ok(timestamp) => {
                    if timestamp.parse::<u128>().is_ok_and(|retired_at| {
                        now_ms.saturating_sub(retired_at) > HTML_CACHE_WINDOW.as_millis()
                    }) && fs::remove_file(entry.path()).is_ok()
                    {
                        let _ = fs::remove_file(marker);
                    }
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    let _ = atomic_write_file(&markers, &marker, now_ms.to_string().as_bytes());
                }
                Err(_) => {}
            }
        }
    }

    pub(super) fn clear(self) {
        let _ = fs::remove_dir_all(self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_theme_and_media_survive_the_cache_window_and_reuse_resets_it() {
        let dir = tempfile::tempdir().unwrap();
        let published = dir.path().join("pub");
        let media = published.join("media");
        fs::create_dir_all(&media).unwrap();
        let retired = RetiredAssets::new(&dir.path().join(".tmp"), "test-menu");
        let old_theme = published.join("theme-old.css");
        let old_image = media.join("old.jpg");
        fs::write(&old_theme, "body {}").unwrap();
        fs::write(&old_image, "image").unwrap();

        let first_retirement = SystemTime::UNIX_EPOCH + Duration::from_secs(5000);
        let deadline = first_retirement + HTML_CACHE_WINDOW;
        let prune_theme =
            |now| retired.prune(&published, "themes", now, |name| name.starts_with("theme-"));
        let prune_image = |now| retired.prune(&media, "media", now, |_| true);

        prune_theme(first_retirement);
        prune_image(first_retirement);
        prune_theme(deadline);
        prune_image(deadline);
        assert!(old_theme.is_file());
        assert!(old_image.is_file());

        // The theme was referenced again before the second retirement.
        retired.keep_live("themes", "theme-old.css").unwrap();
        let second_retirement = deadline + Duration::from_secs(1);
        prune_theme(second_retirement);
        prune_image(second_retirement);
        assert!(old_theme.is_file());
        assert!(!old_image.exists());

        prune_theme(second_retirement + HTML_CACHE_WINDOW + Duration::from_secs(1));
        assert!(!old_theme.exists());
    }
}
