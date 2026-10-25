//! Runtime state (`state.json`): what is installed, the last check, and history.
//!
//! Unlike `config.toml` this file is owned by Headway; users are not expected to edit it.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// How many history entries are kept per app.
pub const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub apps: BTreeMap<String, AppState>,
}

pub const STATE_VERSION: u32 = 1;

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            apps: BTreeMap::new(),
        }
    }
}

impl State {
    /// State of one app (empty if it was never checked).
    pub fn app(&self, id: &str) -> AppState {
        self.apps.get(id).cloned().unwrap_or_default()
    }

    /// Returns a new state with `id` replaced by `f(current)`.
    #[must_use]
    pub fn with_app(self, id: &str, f: impl FnOnce(AppState) -> AppState) -> Self {
        let current = self.app(id);
        let mut apps = self.apps;
        apps.insert(id.to_owned(), f(current));
        Self { apps, ..self }
    }

    #[must_use]
    pub fn without_app(self, id: &str) -> Self {
        let mut apps = self.apps;
        apps.remove(id);
        Self { apps, ..self }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub auto_update_paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<InstalledRef>,
    /// Last recorded payload after changing origins; never used as an update baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_installation: Option<RetainedInstallation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<crate::github::PullRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<CheckResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<LastError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryEntry>,
}

impl AppState {
    /// Appends a history entry, keeping the newest [`HISTORY_LIMIT`].
    #[must_use]
    pub fn record(self, entry: HistoryEntry) -> Self {
        let mut history = self.history;
        history.push(entry);
        let overflow = history.len().saturating_sub(HISTORY_LIMIT);
        history.drain(..overflow);
        Self { history, ..self }
    }

    /// Marks `installed` as the current version and refreshes the cached check around it.
    #[must_use]
    pub fn installed_now(self, installed: InstalledRef) -> Self {
        let last_check = self.last_check.map(|check| check.rebased_on(&installed.id));
        Self {
            installed: Some(installed),
            retained_installation: None,
            pull_request: None,
            last_check,
            last_error: None,
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedInstallation {
    pub repo: String,
    pub installed: InstalledRef,
}

/// The commit or release Headway installed last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledRef {
    /// Commit SHA in commit mode, tag name in release/tag mode.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    /// Human-readable version (`git describe` output or the tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub installed_at: DateTime<Utc>,
    /// Where the artifact was installed, when Headway copied it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    /// This source has no latest-version probe.
    Unknown,
    UpToDate,
    UpdateAvailable,
    /// Nothing recorded as installed yet; `pending` lists recent candidates.
    NotInstalled,
    /// The installed commit is not an ancestor of the branch (force push, reset).
    Diverged,
}

/// A commit or release that could be installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// Commit SHA or tag name.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub checked_at: DateTime<Utc>,
    pub status: UpdateStatus,
    /// Branch that was checked (commit mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<Candidate>,
    /// Candidates newer than the installed one, newest first. May be truncated.
    #[serde(default)]
    pub pending: Vec<Candidate>,
    /// Total number of newer candidates (can exceed `pending.len()`).
    #[serde(default)]
    pub pending_total: u32,
}

impl CheckResult {
    /// The candidate right after the installed one (the oldest pending).
    pub fn next(&self) -> Option<&Candidate> {
        self.pending.last()
    }

    /// Recomputes `pending`/`status` after `installed_id` got installed, without a network call.
    #[must_use]
    pub fn rebased_on(self, installed_id: &str) -> Self {
        let matches = |candidate: &Candidate| {
            candidate.id == installed_id
                || (self.branch.is_some() && ids_match(&candidate.id, installed_id))
        };
        let is_latest = self.latest.as_ref().is_some_and(matches);
        let position = self.pending.iter().position(matches);

        let count = |items: &[Candidate]| u32::try_from(items.len()).unwrap_or(u32::MAX);
        let (pending, status, pending_total) = match (is_latest, position) {
            (true, _) | (false, Some(0)) => (Vec::new(), UpdateStatus::UpToDate, 0),
            (false, Some(index)) => {
                // The installed candidate and everything older drop out of the list.
                let newer = self.pending[..index].to_vec();
                let dropped = count(&self.pending[index..]);
                let total = self
                    .pending_total
                    .saturating_sub(dropped)
                    .max(count(&newer));
                (newer, UpdateStatus::UpdateAvailable, total)
            }
            // Outside the cached window (a rollback, an arbitrary ref): at least the latest is
            // newer. The exact list comes with the next check.
            (false, None) => match &self.latest {
                Some(latest) => (vec![latest.clone()], UpdateStatus::UpdateAvailable, 1),
                None => (self.pending.clone(), self.status, self.pending_total),
            },
        };
        Self {
            status,
            pending,
            pending_total,
            ..self
        }
    }
}

/// Compares known commit ids, including abbreviations. Never use this for tag names.
pub fn ids_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let is_sha = |s: &str| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit());
    is_sha(a) && is_sha(b) && (a.starts_with(b) || b.starts_with(a))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastError {
    pub code: String,
    pub message: String,
    pub at: DateTime<Utc>,
    /// `check` or `update`.
    pub during: String,
}

impl LastError {
    pub fn from_error(err: &Error, during: &str) -> Self {
        Self {
            code: err.code().to_owned(),
            message: err.to_string(),
            at: Utc::now(),
            during: during.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    /// Built or downloaded and installed.
    Update,
    /// Recorded as installed without building.
    Mark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub at: DateTime<Utc>,
    pub action: HistoryAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
