use crate::github::{CheckRun, GitHub};
use crate::repo::RepoId;

use super::{CommitCiState, CommitMetadata};

/// What the CI checks of one commit say.
pub(crate) struct CiSummary {
    pub state: CommitCiState,
    pub checks_count: u32,
    pub url: Option<String>,
    pub notes: Vec<String>,
}

pub(crate) async fn summarize(gh: &GitHub, repo: &RepoId, sha: &str) -> CiSummary {
    let (runs, legacy) = tokio::join!(gh.check_runs(repo, sha), gh.combined_status(repo, sha));
    let mut states = Vec::new();
    let mut incomplete = false;
    let mut notes = Vec::new();
    let mut checks_count = 0;
    match runs {
        Ok(runs) => {
            checks_count = runs.total_count;
            if usize::try_from(runs.total_count).unwrap_or(usize::MAX) > runs.check_runs.len() {
                incomplete = true;
                notes.push("Check-run scan is incomplete (maximum 100 runs).".into());
            }
            states.extend(runs.check_runs.iter().take(100).map(|run| {
                (
                    run_state(run),
                    run.html_url.clone().or_else(|| run.details_url.clone()),
                )
            }));
        }
        Err(error) => {
            incomplete = true;
            notes.push(format!("Check runs unavailable: {error}"));
        }
    }
    match legacy {
        Ok(combined) => {
            checks_count = checks_count.saturating_add(combined.total_count);
            if combined.total_count > 0 {
                let state = status_state(&combined.state);
                let url = combined
                    .statuses
                    .iter()
                    .find(|item| status_state(&item.state) == state)
                    .or_else(|| combined.statuses.first())
                    .and_then(|item| item.target_url.clone());
                states.push((state, url));
            }
        }
        Err(error) => {
            incomplete = true;
            notes.push(format!("Legacy status unavailable: {error}"));
        }
    }
    let ci_state = if incomplete {
        CommitCiState::Unknown
    } else {
        states
            .iter()
            .map(|(state, _)| *state)
            .max_by_key(|state| priority(*state))
            .unwrap_or(CommitCiState::NotRun)
    };
    let url = states
        .iter()
        .find(|(state, url)| *state == ci_state && url.is_some())
        .or_else(|| states.iter().find(|(_, url)| url.is_some()))
        .and_then(|(_, url)| url.clone());
    CiSummary {
        state: ci_state,
        checks_count,
        url,
        notes,
    }
}

pub(super) async fn populate(gh: &GitHub, repo: &RepoId, metadata: &mut CommitMetadata) {
    let summary = summarize(gh, repo, &metadata.sha).await;
    metadata.ci_state = summary.state;
    metadata.ci_url = summary.url;
    metadata.checks_count = summary.checks_count;
    metadata.notes.extend(summary.notes);
}

fn priority(state: CommitCiState) -> u8 {
    match state {
        CommitCiState::NotRun => 0,
        CommitCiState::Success => 1,
        CommitCiState::Pending => 2,
        CommitCiState::Failure => 3,
        CommitCiState::Unknown => 4,
    }
}

fn status_state(state: &str) -> CommitCiState {
    match state {
        "success" => CommitCiState::Success,
        "failure" | "error" => CommitCiState::Failure,
        "pending" => CommitCiState::Pending,
        _ => CommitCiState::Unknown,
    }
}

fn run_state(run: &CheckRun) -> CommitCiState {
    match run.status.as_str() {
        "queued" | "in_progress" | "waiting" | "pending" | "requested" => CommitCiState::Pending,
        "completed" => match run.conclusion.as_deref() {
            Some("success" | "neutral" | "skipped") => CommitCiState::Success,
            Some("failure" | "cancelled" | "timed_out" | "action_required" | "startup_failure") => {
                CommitCiState::Failure
            }
            _ => CommitCiState::Unknown,
        },
        _ => CommitCiState::Unknown,
    }
}
