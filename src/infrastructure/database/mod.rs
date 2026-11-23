pub mod queries;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{
    SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};

use crate::config::{Config, DbSynchronous};
use crate::error::{AppError, AppResult};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Opens the SQLite pool with WAL mode, foreign keys, busy timeout, conservative cache,
/// and durability settings, then runs pending migrations and verifies the SQLite engine version.
pub async fn init_pool(config: &Config) -> AppResult<SqlitePool> {
    std::fs::create_dir_all(config.db_dir())?;
    std::fs::create_dir_all(config.media_dir())?;
    std::fs::create_dir_all(config.published_tmp_dir())?;

    let db_path = config.db_path();
    let pool = open_pool_at(&db_path, config.db_max_connections, config.db_synchronous).await?;

    let version = query_sqlite_version(&pool).await?;
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode;")
        .fetch_one(&pool)
        .await?;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&pool)
        .await?;

    tracing::info!(
        sqlite_version = %version,
        journal_mode = %journal_mode,
        foreign_keys = foreign_keys,
        db_path = %db_path.display(),
        "SQLite database pool initialized"
    );

    MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| AppError::Internal(format!("migration failed: {e}")))?;

    Ok(pool)
}

pub async fn open_pool_at(
    db_path: &Path,
    max_connections: u32,
    synchronous: DbSynchronous,
) -> AppResult<SqlitePool> {
    let sync_mode = match synchronous {
        DbSynchronous::Full => SqliteSynchronous::Full,
        DbSynchronous::Normal => SqliteSynchronous::Normal,
    };

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(sync_mode)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(5000))
        .auto_vacuum(SqliteAutoVacuum::Incremental)
        .pragma("cache_size", "-8000")
        .pragma("temp_store", "MEMORY");

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    Ok(pool)
}

pub async fn query_sqlite_version(pool: &SqlitePool) -> AppResult<String> {
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(pool)
        .await?;
    Ok(version)
}

/// Begins a write transaction with `BEGIN IMMEDIATE` so concurrent writers wait cleanly
/// on `busy_timeout` instead of failing with `SQLITE_BUSY` during read-to-write upgrades.
pub async fn begin_immediate(
    pool: &SqlitePool,
) -> AppResult<sqlx::Transaction<'static, sqlx::Sqlite>> {
    Ok(pool.begin_with("BEGIN IMMEDIATE").await?)
}

/// Safe online backup using SQLite `VACUUM INTO` (works safely while WAL writes occur).
pub async fn backup_sqlite_into(pool: &SqlitePool, dest_path: &Path) -> AppResult<()> {
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dest_path.exists() {
        std::fs::remove_file(dest_path)?;
    }
    let dest_str = dest_path
        .to_str()
        .ok_or_else(|| AppError::Internal("non-utf8 backup path".into()))?;

    sqlx::query("VACUUM INTO ?")
        .bind(dest_str)
        .execute(pool)
        .await?;

    // Verify integrity of the produced backup snapshot.
    let check_opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .map_err(|e| AppError::Internal(e.to_string()))?
        .filename(dest_path)
        .read_only(true);
    let check_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(check_opts)
        .await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check;")
        .fetch_one(&check_pool)
        .await?;
    check_pool.close().await;

    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(AppError::Internal(format!(
            "backup integrity check failed: {integrity}"
        )));
    }
    Ok(())
}

/// Restores a verified SQLite backup file atomically after removing stale WAL/SHM sidecars.
/// Should only be run while the application server is stopped.
pub async fn restore_sqlite_from(backup_file: &Path, target_db_path: &Path) -> AppResult<()> {
    if !backup_file.exists() {
        return Err(AppError::NotFound("error.not_found"));
    }

    // Verify backup before touching the live database file.
    let verify_opts = SqliteConnectOptions::new()
        .filename(backup_file)
        .read_only(true);
    let verify_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(verify_opts)
        .await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check;")
        .fetch_one(&verify_pool)
        .await?;
    verify_pool.close().await;

    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(AppError::Internal(format!(
            "backup integrity check failed: {integrity}"
        )));
    }

    if let Some(parent) = target_db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove WAL/SHM files left by the previous database so they are never paired with restored bytes.
    let wal_path = Path::new(&format!("{}-wal", target_db_path.display())).to_path_buf();
    let shm_path = Path::new(&format!("{}-shm", target_db_path.display())).to_path_buf();
    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
    }
    if shm_path.exists() {
        std::fs::remove_file(&shm_path)?;
    }

    let tmp_restore = Path::new(&format!("{}.restoring", target_db_path.display())).to_path_buf();
    std::fs::copy(backup_file, &tmp_restore)?;
    let file = std::fs::File::open(&tmp_restore)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp_restore, target_db_path)?;

    Ok(())
}
