//! A safe-Rust, same-filesystem intent journal. No paths from disk are used for
//! cleanup: all paths are derived from the caller's validated destination.
use super::{copy_payload, remove_any, validate};
#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Intent {
    name: String,
    had_old: bool,
    committed: bool,
}

pub(crate) struct Transaction {
    dest: PathBuf,
    root: PathBuf,
    intent: Intent,
    armed: bool,
}

pub(crate) fn root(dest: &Path) -> Result<PathBuf> {
    let parent = dest
        .parent()
        .ok_or_else(|| Error::InvalidInput("destination has no parent".into()))?;
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::InvalidInput("invalid destination name".into()))?;
    Ok(parent.join(format!(".{name}.headway-transaction")))
}

impl Transaction {
    pub(crate) fn begin(src: &Path, dest: &Path) -> Result<Self> {
        validate(src, dest)?;
        let parent = dest
            .parent()
            .ok_or_else(|| Error::InvalidInput("destination has no parent".into()))?;
        fs::create_dir_all(parent).map_err(|error| Error::io_at("create", parent, error))?;
        let parent = parent
            .canonicalize()
            .map_err(|error| Error::io_at("resolve", parent, error))?;
        let name = dest
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::InvalidInput("invalid destination name".into()))?
            .to_owned();
        let dest = parent.join(&name);
        let root = root(&dest)?;
        // Atomic reservation: never reconcile another live writer's transaction.
        fs::create_dir(&root)
            .map_err(|error| Error::io_at("reserve installation", &root, error))?;
        let mut transaction = Self {
            intent: Intent {
                name,
                had_old: fs::symlink_metadata(&dest).is_ok(),
                committed: false,
            },
            dest,
            root,
            armed: true,
        };
        transaction.write_intent()?;
        fs::create_dir(transaction.root.join("new"))
            .map_err(|error| Error::io("create staging", error))?;
        fs::create_dir(transaction.root.join("old"))
            .map_err(|error| Error::io("create backup", error))?;
        let staged = transaction.staged();
        copy_payload(src, &staged)?;
        if src.is_file() {
            crate::archive::set_executable(&staged)?;
        }
        sync_tree(&staged)?;
        sync_dir(&transaction.root.join("new"))?;
        sync_dir(&transaction.root)?;
        sync_dir(&parent)?;
        if transaction.intent.had_old {
            fs::rename(&transaction.dest, transaction.old())
                .map_err(|error| Error::io_at("move aside", &transaction.dest, error))?;
            sync_dir(&transaction.root.join("old"))?;
            sync_dir(&parent)?;
        }
        fs::rename(&staged, &transaction.dest)
            .map_err(|error| Error::io_at("install", &transaction.dest, error))?;
        sync_dir(&transaction.root.join("new"))?;
        sync_dir(&parent)?;
        Ok(transaction)
    }

    fn staged(&self) -> PathBuf {
        self.root.join("new").join(&self.intent.name)
    }
    fn old(&self) -> PathBuf {
        self.root.join("old").join(&self.intent.name)
    }
    fn write_intent(&mut self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.intent)
            .map_err(|error| Error::parse("install journal", &error))?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root)
            .map_err(|error| Error::io("create install journal", error))?;
        file.write_all(&bytes)
            .and_then(|()| file.as_file().sync_all())
            .map_err(|error| Error::io("sync install journal", error))?;
        file.persist(self.root.join("intent.json"))
            .map_err(|error| Error::io("save install journal", error.error))?;
        sync_dir(&self.root)
    }
    pub(crate) fn commit(mut self) -> Result<()> {
        self.intent.committed = true;
        self.write_intent()?;
        self.armed = false;
        self.cleanup()
    }
    pub(crate) fn rollback(&mut self) -> Result<()> {
        if self.old().exists() || self.old().is_symlink() {
            validate(&self.old(), &self.dest)?;
            remove_any(&self.dest)?;
            fs::rename(self.old(), &self.dest)
                .map_err(|error| Error::io_at("restore", &self.dest, error))?;
        } else if !self.intent.had_old && !self.staged().exists() {
            remove_any(&self.dest)?;
        }
        sync_dir(
            self.dest
                .parent()
                .ok_or_else(|| Error::InvalidInput("missing parent".into()))?,
        )?;
        self.armed = false;
        self.cleanup()
    }
    fn cleanup(&self) -> Result<()> {
        // root is derived from dest, checked as a directory on load, never a journal-provided path.
        remove_any(&self.root)?;
        sync_dir(
            self.dest
                .parent()
                .ok_or_else(|| Error::InvalidInput("missing parent".into()))?,
        )
    }
    /// Leave durable intent for the service to finalize only after state is saved.
    pub(crate) fn defer(mut self) {
        self.armed = false;
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if self.armed
            && let Err(error) = self.rollback()
        {
            eprintln!("Headway install rollback needs reconciliation: {error}");
        }
    }
}

pub(crate) fn reconcile(dest: &Path, commit: bool) -> Result<()> {
    let root = root(dest)?;
    if !root.exists() && !root.is_symlink() {
        return Ok(());
    }
    if root.is_symlink()
        || !root.is_dir()
        || root.join("intent.json").is_symlink()
        || root.join("old").is_symlink()
        || root.join("new").is_symlink()
    {
        return Err(Error::InvalidInput("unsafe install journal path".into()));
    }
    let file = root.join("intent.json");
    if !file.exists() {
        // The reservation was created but no durable intent was published. No
        // mutation can precede that publication. Never discard an old payload.
        let old = root.join("old");
        if old.exists()
            && fs::read_dir(&old)
                .map_err(|error| Error::io_at("inspect", &old, error))?
                .next()
                .is_some()
        {
            return Err(Error::InvalidInput(
                "install journal is missing but a rescue payload remains".into(),
            ));
        }
        remove_any(&root)?;
        return sync_dir(
            dest.parent()
                .ok_or_else(|| Error::InvalidInput("missing destination parent".into()))?,
        );
    }
    if fs::metadata(&file)
        .map_err(|error| Error::io_at("read", &file, error))?
        .len()
        > 4096
    {
        return Err(Error::InvalidInput("install journal is too large".into()));
    }
    let intent: Intent = serde_json::from_slice(
        &fs::read(&file).map_err(|error| Error::io_at("read", &file, error))?,
    )
    .map_err(|error| Error::parse("install journal", &error))?;
    if dest.file_name().and_then(|name| name.to_str()) != Some(intent.name.as_str()) {
        return Err(Error::InvalidInput(
            "install journal destination mismatch".into(),
        ));
    }
    let mut transaction = Transaction {
        dest: dest.to_owned(),
        root,
        intent,
        armed: false,
    };
    if transaction.intent.committed || commit {
        transaction.commit()
    } else {
        transaction.rollback()
    }
}

pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| Error::io_at("sync", path, error))
}

pub(crate) fn sync_tree(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|error| Error::io_at("read", path, error))?;
    if meta.is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| Error::io_at("read", path, error))? {
            sync_tree(
                &entry
                    .map_err(|error| Error::io("read staging entry", error))?
                    .path(),
            )?;
        }
    }
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| Error::io_at("sync", path, error))
}
