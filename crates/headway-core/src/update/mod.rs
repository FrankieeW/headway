//! Installing one target (a commit or a release) for an app: the build or
//! download pipeline itself. Recording the result is up to [`crate::service`].

#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::config::{Settings, TrackedApp};
use crate::error::{Error, Result};
use crate::github::GitHub;
use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::state::{Candidate, InstalledRef};

mod job;
mod plan;
pub use plan::plan_steps;
mod log;
mod release_target;
pub use log::{LOG_KEEP, RunLog, latest_log};
#[cfg(test)]
use log::{log_files, prune_logs};
pub(crate) use release_target::release_for_target;

/// What to install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// The newest commit on the branch / the newest release or tag.
    Latest,
    /// The candidate right after the installed one (one commit or version at a time).
    Next,
    /// A specific commit SHA, branch or tag.
    Ref(String),
}

impl Target {
    /// `latest`, `next`, or anything else as a ref.
    pub fn parse(input: &str) -> Self {
        match input.trim() {
            "latest" | "" => Self::Latest,
            "next" => Self::Next,
            other => Self::Ref(other.to_owned()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Latest => "the latest version".to_owned(),
            Self::Next => "the next version".to_owned(),
            Self::Ref(reference) => format!("`{reference}`"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetChoice {
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// What `update --dry-run` prints.
#[derive(Debug, Clone, Serialize)]
pub struct UpdatePlan {
    pub app_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<InstalledRef>,
    pub to: Candidate,
    /// The target is what is installed already; a real run would do nothing.
    pub up_to_date: bool,
    /// `--to` the installed version: a real run builds/downloads it again.
    pub reinstall: bool,
    pub method: &'static str,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetChoice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateOutcome {
    pub app_id: String,
    /// `false` when the target was already installed.
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<InstalledRef>,
    pub to: InstalledRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    pub duration_ms: u64,
    /// Integrity evidence that was unavailable; never implies publisher authenticity.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Result of a successful job.
#[derive(Debug, Clone)]
pub struct Installed {
    pub sha: Option<String>,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

/// Everything one update run needs.
pub struct Job<'a> {
    pub app: &'a TrackedApp,
    pub settings: &'a Settings,
    pub paths: &'a Paths,
    pub gh: &'a GitHub,
    pub env: &'a ShellEnv,
    /// Used for git over HTTPS; never handed to build commands.
    pub token: Option<&'a str>,
    pub cancel: &'a CancellationToken,
    pub log: &'a RunLog<'a>,
}

mod artifact;
pub use artifact::resolve_artifact;

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::InvalidInput(format!("{} has no file name", path.display())))
}

/// First 7 characters of a SHA, other refs unchanged.
pub fn short(reference: &str) -> String {
    let is_sha = reference.len() >= 12 && reference.chars().all(|c| c.is_ascii_hexdigit());
    if is_sha {
        reference.chars().take(7).collect()
    } else {
        reference.to_owned()
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024 * 10 && unit < UNITS.len() - 1 {
        value /= 1024;
        unit += 1;
    }
    format!("{value} {}", UNITS[unit])
}

#[cfg(test)]
#[path = "../update_tests.rs"]
mod tests;
