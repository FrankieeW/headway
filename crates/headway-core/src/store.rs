//! Reading and writing `config.toml` and `state.json`.
//!
//! The CLI and the desktop app can run at the same time, so every
//! read-modify-write happens under an exclusive lock on `.store.lock`, and files
//! are replaced atomically (write to a temp file, then rename).

use std::fs::{self, File, OpenOptions};
use std::hash::{Hash as _, Hasher as _};
use std::io::Write as _;
use std::path::Path;
use std::time::SystemTime;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::state::State;

const CONFIG_HEADER: &str = "\
# Headway configuration, shared by the `headway` CLI and the Headway app.
# Edit with `headway add/edit/remove/settings`, in the app, or by hand.
# Saving through Headway rewrites formatting and comments.
# Reference: https://github.com/FrankieeW/headway#configuration

";

#[derive(Debug, Clone)]
pub struct Store {
    paths: Paths,
}

/// Modification times of the two files, used to notice changes made by another process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fingerprint {
    config: Option<(SystemTime, u64, u64)>,
    state: Option<(SystemTime, u64, u64)>,
}

impl Store {
    pub const fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub const fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Loads and validates `config.toml`; a missing file is an empty config.
    pub fn config(&self) -> Result<Config> {
        let path = self.paths.config_file();
        let Some(text) = read_optional(&path)? else {
            return Ok(Config::default());
        };
        let config: Config =
            toml::from_str(&text).map_err(|err| Error::parse(path.display().to_string(), &err))?;
        config.validate()?;
        Ok(config)
    }

    /// Loads `state.json`; a missing file is an empty state.
    pub fn state(&self) -> Result<State> {
        let path = self.paths.state_file();
        let Some(text) = read_optional(&path)? else {
            return Ok(State::default());
        };
        let mut state: State = serde_json::from_str(&text).map_err(|err| Error::parse(
            path.display().to_string(),
            &format!("{err}; installed provenance was not reset: preserve this file; restore the validated state.json.bak beside it if available, or intentionally reset only after accepting loss of installed provenance"),
        ))?;
        if state.version > crate::state::STATE_VERSION {
            return Err(Error::Config(format!(
                "{} uses unsupported state schema {}; upgrade Headway; the file was not modified",
                path.display(),
                state.version
            )));
        }
        state.version = crate::state::STATE_VERSION;
        Ok(state)
    }

    /// Applies `f` to the current config under the store lock and saves the result.
    pub fn update_config<T>(&self, f: impl FnOnce(Config) -> Result<(Config, T)>) -> Result<T> {
        let _lock = self.lock()?;
        let (next, output) = f(self.config()?)?;
        next.validate()?;
        let body = toml::to_string_pretty(&next)
            .map_err(|err| Error::Config(format!("serialize: {err}")))?;
        write_atomic(
            &self.paths.config_file(),
            format!("{CONFIG_HEADER}{body}").as_bytes(),
        )?;
        Ok(output)
    }

    /// Applies `f` to the current state under the store lock and saves the result.
    pub fn update_state<T>(&self, f: impl FnOnce(State) -> Result<(State, T)>) -> Result<T> {
        let _lock = self.lock()?;
        let previous = self.state()?;
        let (next, output) = f(previous.clone())?;
        let body = serde_json::to_vec_pretty(&next)
            .map_err(|err| Error::Config(format!("serialize state: {err}")))?;
        self.backup_state(&previous)?;
        write_atomic(&self.paths.state_file(), &body)?;
        Ok(output)
    }

    /// Rechecks check provenance under the same lock as its state write.
    pub(crate) fn update_state_for_source<T>(
        &self,
        id: &str,
        source: &crate::Source,
        f: impl FnOnce(State) -> Result<(State, T)>,
    ) -> Result<T> {
        let _lock = self.lock()?;
        if &self.config()?.app(id)?.source != source {
            return Err(Error::ConfigChanged(id.into()));
        }
        let previous = self.state()?;
        let (next, output) = f(previous.clone())?;
        let body = serde_json::to_vec_pretty(&next).map_err(|err| Error::parse("state", &err))?;
        self.backup_state(&previous)?;
        write_atomic(&self.paths.state_file(), &body)?;
        Ok(output)
    }

    /// Clears orphan provenance before making new registrations visible. A failed config
    /// write can lose only orphan state, never attach it to a new source after a crash.
    pub(crate) fn register<T>(
        &self,
        f: impl FnOnce(Config, State) -> Result<(Config, State, T)>,
    ) -> Result<T> {
        let _lock = self.lock()?;
        let previous = self.state()?;
        let (config, state, output) = f(self.config()?, previous.clone())?;
        config.validate()?;
        let state_body =
            serde_json::to_vec_pretty(&state).map_err(|err| Error::parse("state", &err))?;
        let config_body =
            toml::to_string_pretty(&config).map_err(|err| Error::parse("config", &err))?;
        if state != previous {
            self.backup_state(&previous)?;
            write_atomic(&self.paths.state_file(), &state_body)?;
        }
        write_atomic(
            &self.paths.config_file(),
            format!("{CONFIG_HEADER}{config_body}").as_bytes(),
        )?;
        Ok(output)
    }

    fn backup_state(&self, state: &State) -> Result<()> {
        let body =
            serde_json::to_vec_pretty(state).map_err(|err| Error::parse("state backup", &err))?;
        write_atomic(&self.paths.state_file().with_extension("json.bak"), &body)
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint {
            config: stamp(&self.paths.config_file()),
            state: stamp(&self.paths.state_file()),
        }
    }

    fn lock(&self) -> Result<File> {
        let path = self.paths.store_lock();
        create_parent(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|err| Error::io_at("open", &path, err))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(Error::StoreBusy(path.display().to_string()));
                }
                Err(std::fs::TryLockError::Error(err)) => {
                    return Err(Error::io_at("lock", &path, err));
                }
            }
        }
        Ok(file)
    }
}

fn stamp(path: &Path) -> Option<(SystemTime, u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fs::read(path).ok()?.hash(&mut hasher);
    Some((meta.modified().ok()?, meta.len(), hasher.finish()))
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::io_at("read", path, err)),
    }
}

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::io_at("create", parent, err))?;
    }
    Ok(())
}

/// Writes `contents` to a sibling temp file and renames it over `path`.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    create_parent(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidInput("atomic file has no parent".into()))?;
    // NamedTempFile uses create_new and mode 0600, and cleans up on every error path.
    let mut tmp = tempfile::Builder::new()
        .prefix(".headway-")
        .tempfile_in(parent)
        .map_err(|err| Error::io_at("create temporary file in", parent, err))?;
    tmp.write_all(contents)
        .map_err(|err| Error::io_at("write", tmp.path(), err))?;
    tmp.as_file()
        .sync_all()
        .map_err(|err| Error::io_at("sync", tmp.path(), err))?;
    tmp.persist(path)
        .map_err(|err| Error::io_at("replace", path, err.error))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|err| Error::io_at("sync directory", parent, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::testing::cli_app;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(Paths::under(dir.path()));
        (dir, store)
    }

    #[test]
    fn missing_files_are_empty() {
        let (_dir, store) = store();
        assert_eq!(store.config().unwrap(), Config::default());
        assert_eq!(store.state().unwrap(), State::default());
    }

    #[test]
    fn config_updates_persist_with_header() {
        let (_dir, store) = store();
        let count = store
            .update_config(|config| {
                let mut apps = config.apps;
                apps.push(cli_app("rg"));
                let count = apps.len();
                Ok((Config { apps, ..config }, count))
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(store.config().unwrap().apps[0].id, "rg");
        let text = fs::read_to_string(store.paths().config_file()).unwrap();
        assert!(text.starts_with("# Headway configuration"));
        assert!(text.contains("[[app]]"));
    }

    #[test]
    fn invalid_updates_are_not_written() {
        let (_dir, store) = store();
        let result = store.update_config(|config| {
            Ok((
                Config {
                    apps: vec![cli_app("rg"), cli_app("rg")],
                    ..config
                },
                (),
            ))
        });
        assert!(result.is_err());
        assert!(!store.paths().config_file().exists());
    }

    #[test]
    fn corrupt_config_reports_the_path() {
        let (_dir, store) = store();
        write_atomic(&store.paths().config_file(), b"[[app]\nid=").unwrap();
        let err = store.config().unwrap_err();
        assert_eq!(err.code(), "parse_error");
        assert!(err.to_string().contains("config.toml"));
    }

    #[test]
    fn state_updates_round_trip_and_change_the_fingerprint() {
        let (_dir, store) = store();
        let before = store.fingerprint();
        store
            .update_state(|state| Ok((state.with_app("rg", |app| AppState { ..app }), ())))
            .unwrap();
        assert!(store.state().unwrap().apps.contains_key("rg"));
        assert_ne!(store.fingerprint(), before);
    }
}

#[cfg(test)]
#[path = "store_regressions.rs"]
mod regressions;
