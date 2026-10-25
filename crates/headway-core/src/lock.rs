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
    legacy: Option<Box<Self>>,
}

impl AppLock {
    /// Locks durable then legacy locations, in that order in every new process.
    /// The legacy guard keeps an already-running older desktop mutually excluded.
    pub fn acquire_app(paths: &crate::paths::Paths, app_id: &str) -> Result<Self> {
        crate::repo::validate_app_id(app_id)?;
        let mut guard = Self::acquire(&paths.app_lock(app_id), app_id)?;
        guard.legacy = Some(Box::new(Self::acquire(
            &paths.legacy_app_lock(app_id),
            app_id,
        )?));
        Ok(guard)
    }

    pub async fn acquire_app_async(paths: &crate::paths::Paths, app_id: &str) -> Result<Self> {
        let paths = paths.clone();
        let app_id = app_id.to_owned();
        tokio::task::spawn_blocking(move || Self::acquire_app(&paths, &app_id))
            .await
            .map_err(|err| Error::io("acquire compatible app locks", std::io::Error::other(err)))?
    }

    /// Acquires without sleeping on an async runtime worker.
    pub async fn acquire_async(path: &Path, app_id: &str) -> Result<Self> {
        let path = path.to_owned();
        let app_id = app_id.to_owned();
        tokio::task::spawn_blocking(move || Self::acquire(&path, &app_id))
            .await
            .map_err(|err| Error::io("acquire app lock", std::io::Error::other(err)))?
    }

    /// Takes the lock or fails with [`Error::Busy`] if another update holds it.
    ///
    /// [`is_locked`] probes hold the lock for an instant, so a would-block is retried
    /// briefly before it is reported as busy.
    pub fn acquire(path: &Path, app_id: &str) -> Result<Self> {
        let file = open(path)?;
        for _ in 0..ACQUIRE_ATTEMPTS {
            match file.try_lock() {
                Ok(()) => return Ok(Self { file, legacy: None }),
                Err(TryLockError::WouldBlock) => std::thread::sleep(RETRY_DELAY),
                Err(TryLockError::Error(err)) => return Err(Error::io_at("lock", path, err)),
            }
        }
        Err(Error::Busy(app_id.to_owned()))
    }
}

impl Drop for AppLock {
    fn drop(&mut self) {
        // Retain primary exclusion until the compatibility guard is released.
        drop(self.legacy.take());
        // A concurrently spawning child can briefly inherit this descriptor before exec.
        // Unlock explicitly so that copy cannot keep an already-finished job busy.
        if let Err(err) = self.file.unlock() {
            eprintln!("headway: release update lock: {err}");
        }
    }
}

/// Busy projection includes old peers that know only the cache lock.
pub fn app_is_locked(paths: &crate::paths::Paths, app_id: &str) -> bool {
    is_locked(&paths.app_lock(app_id)) || is_locked(&paths.legacy_app_lock(app_id))
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
    #[test]
    fn legacy_peer_blocks_new_admission_and_failed_admission_releases_primary() {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths::under(root.path());
        let old = AppLock::acquire(&paths.legacy_app_lock("tool"), "tool").unwrap();
        assert!(app_is_locked(&paths, "tool"));
        assert!(matches!(
            AppLock::acquire_app(&paths, "tool"),
            Err(Error::Busy(_))
        ));
        assert!(!is_locked(&paths.app_lock("tool")));
        drop(old);
        assert!(!app_is_locked(&paths, "tool"));
        AppLock::acquire_app(&paths, "tool").unwrap();
    }

    #[test]
    fn new_guard_is_visible_to_old_peers_and_both_guards_release() {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths::under(root.path());
        let new = AppLock::acquire_app(&paths, "tool").unwrap();
        assert!(is_locked(&paths.app_lock("tool")));
        assert!(is_locked(&paths.legacy_app_lock("tool")));
        assert!(matches!(
            AppLock::acquire(&paths.legacy_app_lock("tool"), "tool"),
            Err(Error::Busy(_))
        ));
        drop(new);
        assert!(!app_is_locked(&paths, "tool"));
        AppLock::acquire(&paths.legacy_app_lock("tool"), "tool").unwrap();
    }

    #[test]
    fn durable_primary_excludes_new_peers_even_after_legacy_cache_purge() {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths::under(root.path());
        let held = AppLock::acquire_app(&paths, "tool").unwrap();
        std::fs::remove_dir_all(&paths.cache_dir).unwrap();
        assert!(matches!(
            AppLock::acquire_app(&paths, "tool"),
            Err(Error::Busy(_))
        ));
        assert!(app_is_locked(&paths, "tool"));
        drop(held);
        assert!(!app_is_locked(&paths, "tool"));
        AppLock::acquire_app(&paths, "tool").unwrap();
    }

    #[tokio::test]
    async fn async_admission_also_honors_an_older_peers_lock() {
        let root = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths::under(root.path());
        let old = AppLock::acquire(&paths.legacy_app_lock("tool"), "tool").unwrap();
        assert!(matches!(
            AppLock::acquire_app_async(&paths, "tool").await,
            Err(Error::Busy(_))
        ));
        drop(old);
        AppLock::acquire_app_async(&paths, "tool").await.unwrap();
    }
}
