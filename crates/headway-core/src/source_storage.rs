//! Source checkout retention, location and ownership for source builds.
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

use crate::config::Install;
use crate::lock::AppLock;
use crate::{Error, Headway, Result};

mod checkout;
mod ownership;
#[cfg(test)]
mod tests;
pub(crate) use checkout::{CheckoutContext, SourceCheckout};
pub(crate) use ownership::Location;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStorage {
    #[serde(default = "keep_sources")]
    pub keep: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

const fn keep_sources() -> bool {
    true
}

impl Default for SourceStorage {
    fn default() -> Self {
        Self {
            keep: true,
            root: None,
        }
    }
}

impl SourceStorage {
    pub fn validate(&self) -> Result<()> {
        if let Some(root) = &self.root {
            let path = Path::new(root);
            if root.is_empty()
                || root.contains('\0')
                || !(path.is_absolute() || root == "~" || root.starts_with("~/"))
                || path
                    .components()
                    .any(|part| matches!(part, Component::ParentDir))
            {
                return Err(Error::InvalidInput(
                    "source storage root must be an absolute path or ~/ path without traversal"
                        .into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStorageStatus {
    /// The retained checkout location; temporary builds use unique siblings.
    pub path: String,
    pub keep: bool,
    pub exists: bool,
    pub custom_root: bool,
}

impl Headway {
    pub fn source_storage_status(&self, id: &str) -> Result<SourceStorageStatus> {
        crate::repo::validate_app_id(id)?;
        let _lock = AppLock::acquire(&self.paths().app_lock(id), id)?;
        let config = self.store().config()?;
        let app = config.app(id)?;
        let github = app.require_github("source storage")?;
        match &github.install {
            Install::Source(source) => Location::new(&source.storage, self.paths(), id)?.status(),
            Install::Asset(_) => Err(Error::InvalidInput(
                "source storage requires a source build".into(),
            )),
        }
    }
}

pub(crate) fn purge(paths: &crate::paths::Paths, app: &crate::TrackedApp) -> Result<()> {
    let Some(github) = app.github() else {
        return Ok(());
    };
    match &github.install {
        Install::Source(source) => Location::new(&source.storage, paths, &app.id)?.purge(),
        Install::Asset(_) => Ok(()),
    }
}
