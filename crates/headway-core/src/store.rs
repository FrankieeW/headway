//! Reading and writing `config.toml` and `state.json`.
//!
//! The CLI and the desktop app can run at the same time, so every
//! read-modify-write happens under an exclusive lock on `.store.lock`, and files
//! are replaced atomically (write to a temp file, then rename).

use std::fs::{self, File, OpenOptions};
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
# Reference: https://github.com/FrankieeW/headway#configuration

";

#[derive(Debug, Clone)]
pub struct Store {
    paths: Paths,
}

/// Modification times of the two files, used to notice changes made by another process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fingerprint {
    config: Option<(SystemTime, u64)>,
    state: Option<(SystemTime, u64)>,
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
        match read_optional(&path)? {
            Some(text) => serde_json::from_str(&text)
                .map_err(|err| Error::parse(path.display().to_string(), &err)),
            None => Ok(State::default()),
        }
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
        let (next, output) = f(self.state()?)?;
        let body = serde_json::to_vec_pretty(&next)
            .map_err(|err| Error::Config(format!("serialize state: {err}")))?;
        write_atomic(&self.paths.state_file(), &body)?;
        Ok(output)
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
        file.lock()
            .map_err(|err| Error::io_at("lock", &path, err))?;
        Ok(file)
    }
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
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
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut file = File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()
    };
    write().map_err(|err| Error::io_at("write", &tmp, err))?;
    fs::rename(&tmp, path).map_err(|err| Error::io_at("replace", path, err))
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
