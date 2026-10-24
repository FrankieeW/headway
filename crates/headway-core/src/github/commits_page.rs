//! Fetches exactly one bounded page; pagination URLs are never followed.
use reqwest::header::LINK;

use super::{CommitInfo, GitHub, JSON, encode, network};
use crate::error::{Error, Result};
use crate::repo::RepoId;
use crate::revision_history::{is_full_sha, validate_page};

/// GitHub wire results for one page, without install/update policy.
#[derive(Debug)]
pub struct CommitListPage {
    pub commits: Vec<CommitInfo>,
    pub has_more: bool,
}

impl GitHub {
    /// Fetches one newest-first page reachable from the exact `anchor` SHA.
    /// `rel=next` is advisory only: no URL from the response is requested.
    ///
    /// # Errors
    /// Rejects invalid pagination/anchors, oversized or invalid responses, and
    /// preserves existing GitHub authentication, rate-limit and network errors.
    pub async fn commits_page(
        &self,
        repo: &RepoId,
        anchor: &str,
        page: u32,
        per_page: u32,
    ) -> Result<CommitListPage> {
        validate_page(page, per_page)?;
        if !is_full_sha(anchor) {
            return Err(Error::InvalidInput(
                "Commit history anchor must be a full 40-character hexadecimal SHA.".into(),
            ));
        }
        let endpoint = format!(
            "/repos/{repo}/commits?sha={}&page={page}&per_page={per_page}",
            encode(anchor)
        );
        let response = self
            .send(
                self.request(&endpoint, JSON),
                &format!("commit page {page} in {repo}"),
            )
            .await?;
        let has_more = response
            .headers()
            .get_all(LINK)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(has_next_relation);
        let commits = read_commits(response).await?;
        if commits.len() > usize::try_from(per_page).unwrap_or(usize::MAX) {
            return Err(Error::Parse {
                what: "GitHub commit page".into(),
                message: "response contains more commits than requested".into(),
            });
        }
        Ok(CommitListPage { commits, has_more })
    }
}

fn has_next_relation(header: &str) -> bool {
    header.split(',').any(|entry| {
        let Some((target, parameters)) = entry.split_once('>') else {
            return false;
        };
        target.trim_start().starts_with('<')
            && parameters.split(';').any(|parameter| {
                let Some((name, value)) = parameter.trim().split_once('=') else {
                    return false;
                };
                name.trim().eq_ignore_ascii_case("rel")
                    && value
                        .trim()
                        .trim_matches('"')
                        .split_ascii_whitespace()
                        .any(|relation| relation == "next")
            })
    })
}

async fn read_commits(mut response: reqwest::Response) -> Result<Vec<CommitInfo>> {
    const MAX_BYTES: usize = 4 * 1024 * 1024;
    let oversized = || Error::Parse {
        what: "GitHub commit page".into(),
        message: "response exceeds the 4 MiB page limit".into(),
    };
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(MAX_BYTES).unwrap_or(u64::MAX))
    {
        return Err(oversized());
    }
    let mut bytes = Vec::with_capacity(MAX_BYTES);
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
            return Err(oversized());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|error| Error::parse("GitHub commit page", &error))
}

#[cfg(test)]
mod tests;
