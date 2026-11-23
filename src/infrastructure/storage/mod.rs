use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Validates a relative file name or single/multi-segment path inside a base directory,
/// rejecting `..`, root prefixes, backslashes, or null bytes.
pub fn resolve_safe_subpath(base: &Path, relative: &str) -> AppResult<PathBuf> {
    if relative.is_empty() || relative.contains('\0') || relative.contains('\\') {
        return Err(AppError::NotFound("error.not_found"));
    }
    let rel = Path::new(relative);
    let mut clean = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg.to_string_lossy();
                if s.starts_with('.') {
                    return Err(AppError::NotFound("error.not_found"));
                }
                clean.push(seg);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::NotFound("error.not_found"));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(base.join(clean))
}

/// Atomically writes `bytes` to `target_path` using a temporary file inside `tmp_dir`
/// on the **same filesystem**, fsyncs the temporary file, and renames it into place.
pub fn atomic_write_file(tmp_dir: &Path, target_path: &Path, bytes: &[u8]) -> AppResult<()> {
    std::fs::create_dir_all(tmp_dir)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut tmp = tempfile::NamedTempFile::new_in(tmp_dir)?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;

    tmp.persist(target_path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.error.kind(),
            format!(
                "atomic persist failed for {}: {}",
                target_path.display(),
                e.error
            ),
        ))
    })?;

    Ok(())
}

/// Writes `bytes` to `target_path` atomically inside its own parent directory.
pub fn atomic_write_in_place(target_path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = target_path
        .parent()
        .ok_or_else(|| AppError::Internal("target path missing parent".into()))?;
    atomic_write_file(parent, target_path, bytes)
}
