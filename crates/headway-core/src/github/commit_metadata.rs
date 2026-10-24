//! Commit CI wire types, separate from update candidates.
use serde::Deserialize;

use super::{GitHub, encode};
use crate::error::Result;
use crate::repo::RepoId;

#[derive(Debug, Deserialize)]
pub struct CheckRuns {
    pub total_count: u32,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRun {
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
    pub details_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CombinedStatus {
    pub state: String,
    pub total_count: u32,
    pub statuses: Vec<CommitStatus>,
}

#[derive(Debug, Deserialize)]
pub struct CommitStatus {
    pub state: String,
    pub target_url: Option<String>,
}

impl GitHub {
    /// At most 100 latest check runs for this exact commit.
    pub async fn check_runs(&self, repo: &RepoId, sha: &str) -> Result<CheckRuns> {
        self.get_json(
            &format!(
                "/repos/{repo}/commits/{}/check-runs?filter=latest&per_page=100",
                encode(sha)
            ),
            &format!("check runs for {sha} in {repo}"),
        )
        .await
    }

    /// Combined legacy status, with at most 100 context details.
    pub async fn combined_status(&self, repo: &RepoId, sha: &str) -> Result<CombinedStatus> {
        self.get_json(
            &format!("/repos/{repo}/commits/{}/status?per_page=100", encode(sha)),
            &format!("combined status for {sha} in {repo}"),
        )
        .await
    }
}
