use super::GitHub;
use crate::{Result, repo::RepoId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRepository {
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullHead {
    #[serde(rename = "ref")]
    pub reference: String,
    pub sha: String,
    pub repo: Option<PullRepository>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    pub head: PullHead,
    pub base: PullHead,
}

impl GitHub {
    pub async fn pull_request(&self, repo: &RepoId, number: u64) -> Result<PullRequest> {
        self.get_json(
            &format!("/repos/{repo}/pulls/{number}"),
            &format!("PR #{number} in {repo}"),
        )
        .await
    }

    pub async fn pull_requests(
        &self,
        repo: &RepoId,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<PullRequest>> {
        self.get_json(&format!("/repos/{repo}/pulls?state=open&sort=updated&direction=desc&page={page}&per_page={per_page}"), &format!("pull requests in {repo}")).await
    }
}
