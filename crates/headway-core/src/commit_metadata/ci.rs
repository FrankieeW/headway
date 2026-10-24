use crate::github::{CheckRun, GitHub};
use crate::repo::RepoId;

use super::{CommitCiState, CommitMetadata};

pub(super) async fn populate(gh: &GitHub, repo: &RepoId, metadata: &mut CommitMetadata) {
    let (runs, legacy) = tokio::join!(
        gh.check_runs(repo, &metadata.sha),
        gh.combined_status(repo, &metadata.sha)
    );
    let mut states = Vec::new();
    let mut incomplete = false;
    match runs {
        Ok(runs) => {
            metadata.checks_count = runs.total_count;
            if usize::try_from(runs.total_count).unwrap_or(usize::MAX) > runs.check_runs.len() {
                incomplete = true;
                metadata
                    .notes
                    .push("Check-run scan is incomplete (maximum 100 runs).".into());
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
            metadata
                .notes
                .push(format!("Check runs unavailable: {error}"));
        }
    }
    match legacy {
        Ok(combined) => {
            metadata.checks_count = metadata.checks_count.saturating_add(combined.total_count);
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
            metadata
                .notes
                .push(format!("Legacy status unavailable: {error}"));
        }
    }
    metadata.ci_state = if incomplete {
        CommitCiState::Unknown
    } else {
        states
            .iter()
            .map(|(state, _)| *state)
            .max_by_key(|state| priority(*state))
            .unwrap_or(CommitCiState::NotRun)
    };
    metadata.ci_url = states
        .iter()
        .find(|(state, url)| *state == metadata.ci_state && url.is_some())
        .or_else(|| states.iter().find(|(_, url)| url.is_some()))
        .and_then(|(_, url)| url.clone());
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
