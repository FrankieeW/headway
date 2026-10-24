use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::digest::digest;
use super::identity::SourceIdentity;
use super::{BackupEntry, component};
use crate::config::TrackedApp;
use crate::error::{Error, Result};
use crate::install;
use crate::paths::Paths;
use crate::state::InstalledRef;

#[derive(Serialize, Deserialize)]
struct Manifest {
    source_identity: SourceIdentity,
    entry: BackupEntry,
    destination: PathBuf,
    digest: String,
}

pub(crate) struct Cache {
    root: PathBuf,
    app_id: String,
    source_identity: SourceIdentity,
}

impl Cache {
    pub fn new(paths: &Paths, app: &TrackedApp) -> Result<Self> {
        Ok(Self {
            root: cache_root(paths, &app.id)?,
            app_id: app.id.clone(),
            source_identity: SourceIdentity::from_app(app)?,
        })
    }

    pub fn list(&self) -> Result<Vec<BackupEntry>> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|manifest| manifest.source_identity == self.source_identity)
            .map(|manifest| manifest.entry)
            .collect())
    }

    fn entries(&self) -> Result<Vec<Manifest>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.root).map_err(|e| Error::io_at("read", &self.root, e))? {
            let item = item.map_err(|e| Error::io("read backup entry", e))?;
            let id = item.file_name().to_string_lossy().into_owned();
            if id.starts_with('.') {
                continue;
            }
            entries.push(self.read_manifest(&id)?);
        }
        entries.sort_by(|a, b| {
            b.entry
                .created_at
                .cmp(&a.entry.created_at)
                .then(b.entry.id.cmp(&a.entry.id))
        });
        Ok(entries)
    }

    pub fn load(&self, id: &str) -> Result<(BackupEntry, PathBuf)> {
        let manifest = self.read_manifest(id)?;
        if manifest.source_identity != self.source_identity {
            return Err(Error::InvalidInput(
                "backup belongs to a different GitHub repository or track mode".into(),
            ));
        }
        Ok((manifest.entry, manifest.destination))
    }

    fn read_manifest(&self, id: &str) -> Result<Manifest> {
        component(id)?;
        let dir = self.root.join(id);
        let metadata = dir.join("metadata.json");
        if dir.is_symlink() || metadata.is_symlink() {
            return Err(Error::InvalidInput(
                "backup metadata cannot be a symlink".into(),
            ));
        }
        let manifest: Manifest = serde_json::from_slice(
            &fs::read(&metadata).map_err(|e| Error::io_at("read", &metadata, e))?,
        )
        .map_err(|e| Error::parse("backup metadata", &e))?;
        manifest.source_identity.validate()?;
        let payload = PathBuf::from(&manifest.entry.artifact_path);
        let canonical_dir = dir
            .canonicalize()
            .map_err(|e| Error::io_at("read", &dir, e))?;
        if manifest.entry.id != id
            || manifest.entry.app_id != self.app_id
            || payload.parent() != Some(canonical_dir.join("payload").as_path())
            || canonical_dir.join("payload").is_symlink()
            || payload.is_symlink()
            || payload.file_name().is_none()
            || payload
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            || !manifest.destination.is_absolute()
            || manifest
                .destination
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::InvalidInput(
                "invalid backup metadata or payload path".into(),
            ));
        }
        if digest(&payload)? != manifest.digest {
            return Err(Error::InvalidInput("backup payload is corrupt".into()));
        }
        Ok(manifest)
    }

    pub fn snapshot(&self, dest: &Path, installed: Option<InstalledRef>) -> Result<Pending> {
        fs::create_dir_all(&self.root).map_err(|e| Error::io_at("create", &self.root, e))?;
        let temp = tempfile::Builder::new()
            .prefix(".pending-")
            .tempdir_in(&self.root)
            .map_err(|e| Error::io_at("create", &self.root, e))?;
        let name = dest
            .file_name()
            .ok_or_else(|| Error::InvalidInput("missing payload name".into()))?;
        let payload_dir = temp.path().join("payload");
        fs::create_dir(&payload_dir).map_err(|e| Error::io_at("create", &payload_dir, e))?;
        let payload = payload_dir.join(name);
        install::copy_payload(dest, &payload)?;
        Ok(Pending {
            temp,
            payload,
            installed,
            destination: dest.to_path_buf(),
        })
    }

    pub fn retain(&self, pending: Pending) -> Result<()> {
        let id = pending
            .temp
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .trim_start_matches('.')
            .to_owned();
        let final_dir = self
            .root
            .canonicalize()
            .map_err(|e| Error::io_at("read", &self.root, e))?
            .join(&id);
        let payload = final_dir
            .join("payload")
            .join(pending.payload.file_name().unwrap_or_default());
        let manifest = Manifest {
            source_identity: self.source_identity.clone(),
            entry: BackupEntry {
                id,
                app_id: self.app_id.clone(),
                created_at: Utc::now(),
                installed: pending.installed,
                artifact_path: payload.display().to_string(),
            },
            destination: pending.destination,
            digest: digest(&pending.payload)?,
        };
        let metadata = pending.temp.path().join("metadata.json");
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| Error::parse("backup metadata", &e))?;
        fs::write(&metadata, bytes).map_err(|e| Error::io_at("write", &metadata, e))?;
        fs::rename(pending.temp.path(), &final_dir)
            .map_err(|e| Error::io_at("retain", &final_dir, e))?;
        for old in self.entries()?.into_iter().skip(2) {
            let path = self.root.join(old.entry.id);
            fs::remove_dir_all(&path).map_err(|e| Error::io_at("prune", &path, e))?;
        }
        Ok(())
    }
}

pub(crate) struct Pending {
    temp: tempfile::TempDir,
    pub payload: PathBuf,
    installed: Option<InstalledRef>,
    destination: PathBuf,
}

impl Pending {
    pub fn preserve(self) -> PathBuf {
        self.temp.keep()
    }
}

fn cache_root(paths: &Paths, id: &str) -> Result<PathBuf> {
    component(id)?;
    let root = paths.cache_dir.join("backups").join(id);
    for path in [
        paths.cache_dir.clone(),
        paths.cache_dir.join("backups"),
        root.clone(),
    ] {
        if path.is_symlink() {
            return Err(Error::InvalidInput(
                "backup directories cannot be symlinks".into(),
            ));
        }
    }
    Ok(root)
}

pub(crate) fn purge(paths: &Paths, id: &str) -> Result<()> {
    let root = cache_root(paths, id)?;
    match fs::remove_dir_all(&root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io_at("purge backups", &root, error)),
    }
}
