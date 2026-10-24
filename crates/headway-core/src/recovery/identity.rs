use serde::{Deserialize, Serialize};

use crate::config::{Track, TrackedApp};
use crate::error::{Error, Result};
use crate::repo::RepoId;

/// Only the origin and ref interpretation define recovery identity. Build recipes,
/// verification commands and branch/filter changes do not change the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SourceIdentity {
    repo: String,
    track_mode: TrackMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TrackMode {
    Commit,
    Release,
    Tag,
}

impl SourceIdentity {
    pub fn from_app(app: &TrackedApp) -> Result<Self> {
        let github = app.require_github("recovery")?;
        let track_mode = match github.track {
            Track::Commit { .. } => TrackMode::Commit,
            Track::Release { .. } => TrackMode::Release,
            Track::Tag { .. } => TrackMode::Tag,
        };
        Ok(Self {
            repo: github.repo_id()?.full_name().to_ascii_lowercase(),
            track_mode,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if RepoId::parse(&self.repo)?.full_name().to_ascii_lowercase() != self.repo {
            return Err(Error::InvalidInput(
                "backup repository identity is not canonical".into(),
            ));
        }
        Ok(())
    }
}
