use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::begin_immediate;
use crate::error::{AppError, AppResult};

type Writers = HashMap<PathBuf, Weak<AsyncMutex<()>>>;
static WRITERS: LazyLock<Mutex<Writers>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Queue Commerce writers in-process instead of racing busy handlers on pooled
/// connections. SQLite's transaction still coordinates other writers and processes.
pub(crate) async fn enter(pool: &SqlitePool) -> AppResult<OwnedMutexGuard<()>> {
    let path = pool.connect_options().get_filename().to_owned();
    let writer = {
        let mut writers = WRITERS
            .lock()
            .map_err(|_| AppError::Internal("Commerce write queue unavailable".into()))?;
        if let Some(writer) = writers.get(&path).and_then(Weak::upgrade) {
            writer
        } else {
            writers.retain(|_, writer| writer.strong_count() != 0);
            let writer = Arc::new(AsyncMutex::new(()));
            writers.insert(path, Arc::downgrade(&writer));
            writer
        }
    };
    Ok(writer.lock_owned().await)
}

pub(crate) async fn begin(
    pool: &SqlitePool,
) -> AppResult<(OwnedMutexGuard<()>, Transaction<'static, Sqlite>)> {
    let turn = enter(pool).await?;
    let tx = begin_immediate(pool).await?;
    Ok((turn, tx))
}
