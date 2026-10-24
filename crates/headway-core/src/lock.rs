//! Per-app locks so one app is never updated twice at the same time, even across
//! processes (CLI and desktop app). Backed by `flock`, released when dropped.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

use crate::error::{Error, Result};

const ACQUIRE_ATTEMPTS: u32 = 10;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(20);

#[derive(Debug)]
pub struct AppLock {
    file: File,
}

impl AppLock {
    /// Takes the lock or fails with [`Error::Busy`] if another update holds it.
    ///
    /// [`is_locked`] probes hold the lock for an instant, so a would-block is retried
    /// briefly before it is reported as busy.
    pub fn acquire(path: &Path, app_id: &str) -> Result<Self> {
        let file = open(path)?;
        for _ in 0..ACQUIRE_ATTEMPTS {
            match file.try_lock() {
                Ok(()) => return Ok(Self { file }),
                Err(TryLockError::WouldBlock) => std::thread::sleep(RETRY_DELAY),
                Err(TryLockError::Error(err)) => return Err(Error::io_at("lock", path, err)),
            }
        }
        Err(Error::Busy(app_id.to_owned()))
    }
}

impl Drop for AppLock {
    fn drop(&mut self) {
        // A concurrently spawning child can briefly inherit this descriptor before exec.
        // Unlock explicitly so that copy cannot keep an already-finished job busy.
        if let Err(err) = self.file.unlock() {
            eprintln!("headway: release update lock: {err}");
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

    #[test]
    fn dropping_the_guard_releases_a_lock_even_when_a_descriptor_copy_survives() {
        // Given the extra descriptor a concurrently spawned child may inherit.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool.lock");
        let held = AppLock::acquire(&path, "tool").unwrap();
        let inherited = held.file.try_clone().unwrap();
        // When the owning operation finishes.
        drop(held);
        // Then its logical lock ends immediately, before the child closes its copy.
        assert!(!is_locked(&path));
        drop(inherited);
    }
}
