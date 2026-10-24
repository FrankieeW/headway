//! Retained payloads and verified replacement for managed GitHub installations.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{Install, TrackedApp};
use crate::error::{Error, Result};
use crate::state::InstalledRef;

mod cache;
mod digest;
mod identity;
#[cfg(test)]
mod tests;
mod transaction;
pub(crate) use cache::{Cache, purge};
pub(crate) use transaction::Replacement;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub id: String,
    pub app_id: String,
    pub created_at: DateTime<Utc>,
    pub installed: Option<InstalledRef>,
    pub artifact_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOutcome {
    pub app_id: String,
    pub backup_id: String,
    pub installed: Option<InstalledRef>,
    pub restored_path: String,
}

pub(crate) fn verification(app: &TrackedApp) -> Result<Option<&str>> {
    let github = app.require_github("retained artifacts and recovery")?;
    match &github.install {
        Install::Source(source) => {
            if source.artifact.is_none() {
                return Err(Error::InvalidInput(
                    "recovery requires a Headway-managed artifact; self-installing builds are unsupported".into(),
                ));
            }
            Ok(source.verify_command.as_deref())
        }
        Install::Asset(asset) => Ok(asset.verify_command.as_deref()),
    }
}

pub(crate) fn component(id: &str) -> Result<()> {
    if !id.bytes().next().is_some_and(|c| c.is_ascii_alphanumeric())
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    {
        return Err(Error::InvalidInput("invalid recovery identifier".into()));
    }
    Ok(())
}
