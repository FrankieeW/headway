//! Bounded commit-history pages anchored to an immutable Git commit.
use serde::{Deserialize, Serialize};

use crate::commit_metadata::CommitCiState;
use crate::{Candidate, Error, Result};

/// One newest-first page; pass `anchor` back unchanged for subsequent pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitPage {
    pub repo: String,
    pub branch: String,
    pub anchor: String,
    pub page: u32,
    pub per_page: u32,
    pub commits: Vec<CommitEntry>,
    pub has_more: bool,
    pub next_page: Option<u32>,
    /// The filter that produced this page.
    #[serde(default)]
    pub filter: CommitFilter,
    /// Commits of this page the filter removed.
    #[serde(default)]
    pub filtered_out: u32,
    /// Incomplete or unavailable tag/release/CI metadata can omit filter matches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// One commit of a history page, with the release it published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitEntry {
    #[serde(flatten)]
    pub candidate: Candidate,
    /// Tags pointing at this commit, newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The published release for this commit, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
    /// The release publishes an asset this machine can install.
    #[serde(default)]
    pub can_download: bool,
    /// The commit's CI state; only read when the caller asked for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_state: Option<CommitCiState>,
}

/// How a page of commits is narrowed before it is returned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitFilter {
    /// Keep only commits that published a release.
    #[serde(default)]
    pub releases_only: bool,
    /// Keep only commits carrying a tag that matches this regular expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Keep only commits whose CI state matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<CommitCiState>,
}

impl CommitFilter {
    /// Whether the filter keeps every commit of a page.
    pub fn is_empty(&self) -> bool {
        !self.releases_only && self.tag.is_none() && self.ci.is_none()
    }
}

pub(crate) fn validate_page(page: u32, per_page: u32) -> Result<()> {
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(Error::InvalidInput(
            "Pagination requires page >= 1 and per_page between 1 and 100.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn is_full_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}
