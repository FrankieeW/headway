//! Per-app locks so one app is never updated twice at the same time, even across
//! processes (CLI and desktop app). Backed by `flock`, released when dropped.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct AppLock {
    _file: File,
}

impl AppLock {
    /// Takes the lock or fails with [`Error::Busy`] if another update holds it.
    pub fn acquire(path: &Path, app_id: &str) -> Result<Self> {
        let file = open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(Error::Busy(app_id.to_owned())),
            Err(TryLockError::Error(err)) => Err(Error::io_at("lock", path, err)),
        }
    }
}

/// Whether some process currently holds the lock at `path`.
pub fn is_locked(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(file) = open(path) else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => false,
    }
}

fn open(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::io_at("create", parent, err))?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|err| Error::io_at("open", path, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_busy_until_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locks/rg.lock");
        assert!(!is_locked(&path));

        let held = AppLock::acquire(&path, "rg").unwrap();
        assert!(is_locked(&path));
        assert!(matches!(AppLock::acquire(&path, "rg"), Err(Error::Busy(id)) if id == "rg"));

        drop(held);
        assert!(!is_locked(&path));
        AppLock::acquire(&path, "rg").unwrap();
    }
}
