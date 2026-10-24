use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use super::{SourceStorage, SourceStorageStatus};
use crate::paths::{Paths, expand_tilde};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub(crate) struct Location {
    pub base: PathBuf,
    pub path: PathBuf,
    pub keep: bool,
    pub custom_root: bool,
    marker: PathBuf,
    owner: Owner,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    format: u32,
    app_id: String,
    checkout: PathBuf,
    config_dir: PathBuf,
}

impl Location {
    pub fn new(storage: &SourceStorage, paths: &Paths, id: &str) -> Result<Self> {
        storage.validate()?;
        crate::repo::validate_app_id(id)?;
        let base = checked(
            &storage
                .root
                .as_deref()
                .map_or_else(|| paths.cache_dir.join("sources"), expand_tilde),
        )?;
        let path = checked(&base.join(id))?;
        let owner = Owner {
            format: 1,
            app_id: id.into(),
            checkout: path.clone(),
            config_dir: checked(&paths.config_dir)?,
        };
        Ok(Self {
            marker: base.join(format!(".{id}.headway-source.json")),
            base,
            path,
            keep: storage.keep,
            custom_root: storage.root.is_some(),
            owner,
        })
    }

    pub fn owned(&self) -> Result<bool> {
        let metadata = match fs::symlink_metadata(&self.marker) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(Error::io_at("read ownership", &self.marker, error)),
        };
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(Error::InvalidInput(
                "source ownership record must be a regular file".into(),
            ));
        }
        let owner: Owner = serde_json::from_slice(
            &fs::read(&self.marker)
                .map_err(|error| Error::io_at("read ownership", &self.marker, error))?,
        )
        .map_err(|error| Error::parse("source ownership", &error))?;
        if owner != self.owner {
            return Err(Error::InvalidInput(
                "source checkout belongs to a different Headway store or app".into(),
            ));
        }
        Ok(true)
    }

    pub fn claim(&self) -> Result<()> {
        fs::create_dir_all(&self.base)
            .map_err(|error| Error::io_at("create", &self.base, error))?;
        checked(&self.base)?;
        let bytes = serde_json::to_vec(&self.owner)
            .map_err(|error| Error::parse("source ownership", &error))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.marker)
            .map_err(|error| Error::io_at("claim source", &self.marker, error))?;
        file.write_all(&bytes)
            .map_err(|error| Error::io_at("write ownership", &self.marker, error))
    }

    pub fn empty(&self) -> Result<bool> {
        if !self.path.exists() {
            return Ok(true);
        }
        Ok(fs::read_dir(&self.path)
            .map_err(|error| Error::io_at("read", &self.path, error))?
            .next()
            .transpose()
            .map_err(|error| Error::io_at("read", &self.path, error))?
            .is_none())
    }

    pub fn status(&self) -> Result<SourceStorageStatus> {
        super::checkout::validate_git_directory(&self.path)?;
        if !self.owned()?
            && !self.empty()?
            && (self.custom_root || !self.path.join(".git").is_dir())
        {
            return Err(self.unowned());
        }
        Ok(SourceStorageStatus {
            path: self.path.display().to_string(),
            keep: self.keep,
            exists: self.path.exists(),
            custom_root: self.custom_root,
        })
    }

    pub fn unowned(&self) -> Error {
        Error::InvalidInput(format!(
            "refusing unowned nonempty source directory {}; select another root or move that directory",
            self.path.display()
        ))
    }

    pub fn purge(&self) -> Result<()> {
        if !self.owned()? {
            return if self.path.exists() {
                Err(self.unowned())
            } else {
                Ok(())
            };
        }
        checked(&self.path)?;
        match fs::remove_dir_all(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io_at("purge source", &self.path, error)),
        }
        fs::remove_file(&self.marker)
            .map_err(|error| Error::io_at("remove ownership", &self.marker, error))
    }
}

/// Resolve platform /var and /tmp aliases, but refuse user-controlled symlinks.
pub(super) fn checked(path: &Path) -> Result<PathBuf> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::InvalidInput(
            "source path cannot traverse parent directories".into(),
        ));
    }
    let absolute =
        std::path::absolute(path).map_err(|error| Error::io_at("resolve", path, error))?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_symlink() => {
                if cfg!(target_os = "macos")
                    && (current == Path::new("/var") || current == Path::new("/tmp"))
                {
                    current = current
                        .canonicalize()
                        .map_err(|error| Error::io_at("resolve", &current, error))?;
                } else {
                    return Err(Error::InvalidInput(format!(
                        "source path contains a symlink: {}",
                        current.display()
                    )));
                }
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(Error::InvalidInput(format!(
                    "source path is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io_at("inspect", &current, error)),
        }
    }
    Ok(current)
}
