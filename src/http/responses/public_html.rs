use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, Metadata};
use std::io::{Error, ErrorKind, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use sha2::{Digest, Sha256};

const MAX_ENTRIES: usize = 512;
const MAX_BYTES: usize = 24 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum HtmlKind {
    Menu,
    Storefront,
}

#[derive(Default)]
pub(crate) struct PublicHtmlCache(Mutex<Entries>);

#[derive(Default)]
struct Entries {
    files: HashMap<PathBuf, Arc<PublicHtml>>,
    insertion_order: VecDeque<PathBuf>,
    bytes: usize,
}

pub(crate) struct PublicHtml {
    pub(crate) body: Bytes,
    pub(crate) etag: String,
    identity: FileIdentity,
}

#[derive(PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl From<&Metadata> for FileIdentity {
    fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}

impl PublicHtmlCache {
    pub(crate) async fn read(
        &self,
        path: PathBuf,
        kind: HtmlKind,
    ) -> std::io::Result<Arc<PublicHtml>> {
        if let Some(page) = self.current(&path) {
            return Ok(page);
        }

        let (path, page) = tokio::task::spawn_blocking(move || {
            let mut file = File::open(&path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(Error::from(ErrorKind::IsADirectory));
            }
            // Metadata and body come from the same open file across an atomic rename.
            let mut body = Vec::new();
            file.read_to_end(&mut body)?;
            let etag = match kind {
                HtmlKind::Menu => menu_etag(&metadata),
                HtmlKind::Storefront => storefront_etag(&body),
            };
            let page = Arc::new(PublicHtml {
                body: body.into(),
                etag,
                identity: FileIdentity::from(&metadata),
            });
            Ok::<_, Error>((path, page))
        })
        .await
        .map_err(Error::other)??;

        self.remember(path, &page);
        Ok(page)
    }

    fn current(&self, path: &Path) -> Option<Arc<PublicHtml>> {
        let page = self.0.lock().ok()?.files.get(path).cloned()?;
        // Check the local filesystem entry before serving cached bytes. This covers a
        // publish, unpublish, or restore performed by an independent process.
        let metadata = fs::metadata(path).ok()?;
        (metadata.is_file() && FileIdentity::from(&metadata) == page.identity).then_some(page)
    }

    fn remember(&self, path: PathBuf, page: &Arc<PublicHtml>) {
        let size = page.body.len();
        if size > MAX_PAGE_BYTES {
            return;
        }
        let Ok(mut entries) = self.0.lock() else {
            return;
        };
        if let Some(replaced) = entries.files.remove(&path) {
            entries.bytes -= replaced.body.len();
            entries.insertion_order.retain(|previous| previous != &path);
        }
        while entries.files.len() >= MAX_ENTRIES || entries.bytes + size > MAX_BYTES {
            let oldest = entries.insertion_order.pop_front().unwrap();
            entries.bytes -= entries.files.remove(&oldest).unwrap().body.len();
        }
        entries.bytes += size;
        entries.insertion_order.push_back(path.clone());
        entries.files.insert(path, Arc::clone(page));
    }
}

pub(crate) fn menu_etag(metadata: &Metadata) -> String {
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("\"{}-{}-{mtime_ns}\"", metadata.ino(), metadata.size())
}

pub(crate) fn storefront_etag(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    format!("\"sf-{}\"", hex::encode(&digest[..8]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::storage::atomic_write_in_place;

    #[tokio::test]
    async fn checks_cached_index_across_external_replace_and_unpublish() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.html");
        let cache = PublicHtmlCache::default();
        atomic_write_in_place(&path, b"old version").unwrap();

        let first = cache.read(path.clone(), HtmlKind::Menu).await.unwrap();
        let cached = cache.read(path.clone(), HtmlKind::Menu).await.unwrap();
        assert!(Arc::ptr_eq(&first, &cached));

        atomic_write_in_place(&path, b"new version").unwrap();
        let updated = cache.read(path.clone(), HtmlKind::Menu).await.unwrap();
        assert_eq!(&updated.body[..], b"new version");
        assert_ne!(updated.etag, first.etag);

        fs::remove_file(&path).unwrap();
        let result = cache.read(path, HtmlKind::Menu).await;
        assert!(matches!(result, Err(error) if error.kind() == ErrorKind::NotFound));
    }
}
