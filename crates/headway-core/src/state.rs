//! Runtime state (`state.json`): what is installed, the last check, and history.
//!
//! Unlike `config.toml` this file is owned by Headway; users are not expected to edit it.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// How many history entries are kept per app.
pub const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub apps: BTreeMap<String, AppState>,
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
        Self { apps }
    }

    #[must_use]
    pub fn without_app(self, id: &str) -> Self {
        let mut apps = self.apps;
        apps.remove(id);
        Self { apps }
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
        let is_latest = self
            .latest
            .as_ref()
            .is_some_and(|latest| ids_match(&latest.id, installed_id));
        let position = self
            .pending
            .iter()
            .position(|candidate| ids_match(&candidate.id, installed_id));

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

/// Commit ids may be abbreviated on one side; tags must match exactly.
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
mod tests {
    use super::*;

    fn candidate(id: &str) -> Candidate {
        Candidate {
            id: id.into(),
            sha: None,
            title: format!("commit {id}"),
            body: None,
            author: None,
            date: None,
            url: None,
            prerelease: false,
        }
    }

    fn check(pending: &[&str], latest: &str) -> CheckResult {
        CheckResult {
            checked_at: Utc::now(),
            status: UpdateStatus::UpdateAvailable,
            branch: Some("main".into()),
            latest: Some(candidate(latest)),
            pending: pending.iter().map(|id| candidate(id)).collect(),
            pending_total: u32::try_from(pending.len()).unwrap(),
        }
    }

    #[test]
    fn rebasing_on_the_latest_clears_pending() {
        let rebased = check(&["ccc", "bbb", "aaa"], "ccc").rebased_on("ccc");
        assert_eq!(rebased.status, UpdateStatus::UpToDate);
        assert!(rebased.pending.is_empty());
        assert_eq!(rebased.pending_total, 0);
    }

    #[test]
    fn rebasing_in_the_middle_keeps_newer_candidates() {
        let rebased = check(&["ccc", "bbb", "aaa"], "ccc").rebased_on("bbb");
        assert_eq!(rebased.status, UpdateStatus::UpdateAvailable);
        assert_eq!(rebased.pending, vec![candidate("ccc")]);
        assert_eq!(rebased.pending_total, 1);
        assert_eq!(rebased.next().unwrap().id, "ccc");
    }

    #[test]
    fn rebasing_outside_the_window_reports_the_latest() {
        let rebased = check(&[], "ccc").rebased_on("zzz");
        assert_eq!(rebased.status, UpdateStatus::UpdateAvailable);
        assert_eq!(rebased.pending, vec![candidate("ccc")]);
        assert_eq!(rebased.pending_total, 1);
    }

    #[test]
    fn short_and_full_shas_match() {
        assert!(ids_match("abcdef1234567890", "abcdef1"));
        assert!(ids_match("abcdef1", "abcdef1234567890"));
        assert!(!ids_match("abcdef1", "abcdef2"));
        assert!(!ids_match("v1.2", "v1.2.0"), "tags must match exactly");
        assert!(!ids_match("abc", "abcdef1"), "too short to be a sha prefix");
    }

    #[test]
    fn history_is_capped() {
        let entry = |n: usize| HistoryEntry {
            at: Utc::now(),
            action: HistoryAction::Update,
            from: None,
            to: n.to_string(),
            version: None,
            ok: true,
            message: None,
            log_path: None,
            duration_ms: None,
        };
        let state =
            (0..HISTORY_LIMIT + 5).fold(AppState::default(), |state, n| state.record(entry(n)));
        assert_eq!(state.history.len(), HISTORY_LIMIT);
        assert_eq!(state.history[0].to, "5");
    }

    #[test]
    fn with_app_returns_a_new_state() {
        let state = State::default().with_app("rg", |app| AppState {
            last_error: Some(LastError {
                code: "x".into(),
                message: "boom".into(),
                at: Utc::now(),
                during: "check".into(),
            }),
            ..app
        });
        assert!(state.app("rg").last_error.is_some());
        assert_eq!(state.app("missing"), AppState::default());
        assert!(state.without_app("rg").apps.is_empty());
    }

    #[test]
    fn installed_now_clears_errors_and_rebases() {
        let app = AppState {
            last_check: Some(check(&["bbb", "aaa"], "bbb")),
            last_error: Some(LastError {
                code: "command_failed".into(),
                message: "boom".into(),
                at: Utc::now(),
                during: "update".into(),
            }),
            ..AppState::default()
        };
        let installed = InstalledRef {
            id: "bbb".into(),
            sha: Some("bbb".into()),
            version: None,
            installed_at: Utc::now(),
            path: None,
        };
        let next = app.installed_now(installed.clone());
        assert_eq!(next.installed, Some(installed));
        assert!(next.last_error.is_none());
        assert_eq!(next.last_check.unwrap().status, UpdateStatus::UpToDate);
    }

    #[test]
    fn json_round_trip() {
        let state = State::default().with_app("rg", |app| AppState {
            last_check: Some(check(&["b"], "b")),
            ..app
        });
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<State>(&text).unwrap(), state);
    }
}
