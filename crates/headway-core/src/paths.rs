//! Filesystem locations shared by the CLI and the desktop app.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};

/// Directory name under the platform config, cache and log roots.
pub const APP_DIR_NAME: &str = "Headway";
/// When set, every location lives under `$HEADWAY_HOME/{config,cache,logs}`.
pub const HOME_ENV: &str = "HEADWAY_HOME";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Paths {
    /// `config.toml` and `state.json`.
    pub config_dir: PathBuf,
    /// Source checkouts, downloads and locks; safe to delete.
    pub cache_dir: PathBuf,
    /// One directory of update logs per app.
    pub log_dir: PathBuf,
}

impl Paths {
    /// Platform defaults (`~/Library/Application Support/Headway` …), or `$HEADWAY_HOME`.
    pub fn from_env() -> Result<Self> {
        if let Some(root) = std::env::var_os(HOME_ENV).filter(|value| !value.is_empty()) {
            return Ok(Self::under(PathBuf::from(root)));
        }
        let missing =
            |what: &str| Error::Config(format!("cannot locate the user {what} directory"));
        let config = dirs::config_dir().ok_or_else(|| missing("config"))?;
        let cache = dirs::cache_dir().ok_or_else(|| missing("cache"))?;
        let logs = if cfg!(target_os = "macos") {
            dirs::home_dir()
                .ok_or_else(|| missing("home"))?
                .join("Library/Logs")
        } else {
            dirs::data_local_dir().ok_or_else(|| missing("data"))?
        };
        Ok(Self {
            config_dir: config.join(APP_DIR_NAME),
            cache_dir: cache.join(APP_DIR_NAME),
            log_dir: logs.join(APP_DIR_NAME),
        })
    }

    /// Everything under one root (tests, sandboxes, `HEADWAY_HOME`).
    pub fn under(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            log_dir: root.join("logs"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn state_file(&self) -> PathBuf {
        self.config_dir.join("state.json")
    }

    pub fn store_lock(&self) -> PathBuf {
        self.config_dir.join(".store.lock")
    }

    pub fn app_lock(&self, id: &str) -> PathBuf {
        self.cache_dir.join("locks").join(format!("{id}.lock"))
    }

    /// Git checkout used for source builds; kept between updates for incremental builds.
    pub fn source_dir(&self, id: &str) -> PathBuf {
        self.cache_dir.join("sources").join(id)
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }

    pub fn app_log_dir(&self, id: &str) -> PathBuf {
        self.log_dir.join(id)
    }
}

/// Expands a leading `~` or `~/` to the home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    let home = || dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// Replaces the home directory prefix with `~` for display.
pub fn display_path(path: &Path) -> String {
    match dirs::home_dir() {
        Some(home) => match path.strip_prefix(&home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_under_root() {
        let paths = Paths::under("/tmp/hw");
        assert_eq!(
            paths.config_file(),
            PathBuf::from("/tmp/hw/config/config.toml")
        );
        assert_eq!(
            paths.state_file(),
            PathBuf::from("/tmp/hw/config/state.json")
        );
        assert_eq!(
            paths.source_dir("rg"),
            PathBuf::from("/tmp/hw/cache/sources/rg")
        );
        assert_eq!(
            paths.app_lock("rg"),
            PathBuf::from("/tmp/hw/cache/locks/rg.lock")
        );
        assert_eq!(paths.app_log_dir("rg"), PathBuf::from("/tmp/hw/logs/rg"));
    }

    #[test]
    fn tilde_expansion() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/.local/bin"), home.join(".local/bin"));
        assert_eq!(
            expand_tilde("/Applications"),
            PathBuf::from("/Applications")
        );
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
        assert_eq!(display_path(&home.join(".local/bin")), "~/.local/bin");
        assert_eq!(display_path(Path::new("/Applications")), "/Applications");
    }
}
