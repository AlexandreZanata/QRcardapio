use std::collections::HashMap;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::error::{AppError, AppResult};

#[derive(Clone, Default)]
pub struct PublishLocks {
    inner: Arc<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

pub(in crate::application) struct PublicationGuard {
    _file: File,
    _local: OwnedMutexGuard<()>,
}

impl PublishLocks {
    pub fn lock_for(&self, est_id: &str) -> Arc<AsyncMutex<()>> {
        let mut guard = self.inner.lock().expect("publish lock map poisoned");
        guard
            .entry(est_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Serialize CLI and HTTP publishers sharing a data directory. Keep the async guard
    /// in the blocking task so cancelled requests cannot queue more waiters for this tenant.
    pub(in crate::application) async fn lock_for_publication(
        &self,
        staging_dir: &Path,
        est_id: &str,
    ) -> AppResult<PublicationGuard> {
        let local = self.lock_for(est_id).lock_owned().await;
        let dir = staging_dir.join("publish-locks");
        let name = lock_filename(est_id);
        let guard = tokio::task::spawn_blocking(move || {
            let file = open_lock_file(&dir, &name)?;
            file.lock()?;
            Ok::<_, io::Error>(PublicationGuard {
                _file: file,
                _local: local,
            })
        })
        .await
        .map_err(|error| AppError::Internal(format!("publication lock task failed: {error}")))??;
        Ok(guard)
    }
}

fn lock_filename(est_id: &str) -> String {
    hex::encode(Sha256::digest(est_id.as_bytes()))
}

fn open_lock_file(dir: &Path, name: &str) -> io::Result<File> {
    create_dir_all(dir)?;
    // Keep the file in place so all waiters use the same inode.
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(name))
}

#[cfg(test)]
mod tests {
    use std::fs::TryLockError;
    use std::future::{Future, poll_fn};
    use std::task::Poll;
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn independent_publish_locks_wait_for_the_same_tenant_only() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let staging = tempfile::tempdir().unwrap();
            let cli = PublishLocks::default();
            let app = PublishLocks::default();
            let cli_guard = cli.lock_for_publication(staging.path(), "a").await.unwrap();

            let probe =
                open_lock_file(&staging.path().join("publish-locks"), &lock_filename("a")).unwrap();
            assert!(matches!(probe.try_lock(), Err(TryLockError::WouldBlock)));

            let mut waiting = std::pin::pin!(app.lock_for_publication(staging.path(), "a"));
            poll_fn(|context| {
                assert!(waiting.as_mut().poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
            let _other_tenant = app.lock_for_publication(staging.path(), "b").await.unwrap();

            drop(cli_guard);
            let _app_guard = waiting.await.unwrap();
            assert!(matches!(probe.try_lock(), Err(TryLockError::WouldBlock)));
        })
        .await
        .unwrap();
    }
}
