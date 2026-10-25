//! Minimal GitHub REST client for the endpoints Headway uses.
//!
//! Only GET requests are issued. Wire types live in [`models`], HTTP policy
//! (base-URL validation, byte caps, retries, deadlines, error redaction) in
//! [`http`], release-asset streaming in [`download`] and bounded list pagination
//! in [`pagination`].

mod bounded;
mod commit_metadata;
mod commits_page;
mod download;
mod http;
mod models;
mod pagination;
mod redact;
mod search;
pub use search::SearchRepository;
#[cfg(test)]
mod search_tests;

pub use bounded::BoundedFileText;
pub use commit_metadata::{CheckRun, CheckRuns, CombinedStatus, CommitStatus};
pub use commits_page::CommitListPage;
pub use http::DEFAULT_API;
pub use models::{
    Account, Asset, Branch, CommitDetail, CommitInfo, Comparison, ContentEntry, GitActor,
    RateLimit, RelatedRepo, Release, RepoInfo, Tag, TagCommit, User,
};
pub use pagination::BoundedList;

#[path = "github_pulls.rs"]
mod pulls;
pub use pulls::{PullHead, PullRepository, PullRequest};

use std::fmt::Write as _;

use crate::error::{Error, Result};
use crate::repo::RepoId;

use http::{CONNECT_TIMEOUT, READ_TIMEOUT};
use models::RateLimitResponse;
use redact::display_text;

/// Default upper bound for [`GitHub::file_text`], which has no caller-provided limit.
const MAX_FILE_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct GitHub {
    http: reqwest::Client,
    api: String,
    token: Option<String>,
}

impl GitHub {
    pub fn new(api: &str, token: Option<String>) -> Result<Self> {
        let api = http::validate_api_base(api)?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("headway/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(redact::network)?;
        Ok(Self { http, api, token })
    }

    pub const fn has_token(&self) -> bool {
        self.token.is_some()
    }

    pub async fn user(&self) -> Result<User> {
        self.get_json("/user", "the authenticated user").await
    }

    pub async fn rate_limit(&self) -> Result<RateLimit> {
        let response: RateLimitResponse = self.get_json("/rate_limit", "the rate limit").await?;
        Ok(response.rate)
    }

    pub async fn repo(&self, repo: &RepoId) -> Result<RepoInfo> {
        self.get_json(&format!("/repos/{repo}"), &format!("repository {repo}"))
            .await
    }

    /// One commit by SHA, branch or tag.
    pub async fn commit(&self, repo: &RepoId, reference: &str) -> Result<CommitInfo> {
        self.get_json(
            &format!("/repos/{repo}/commits/{}", encode(reference)),
            &format!("commit `{}` in {repo}", display_text(reference)),
        )
        .await
    }

    /// The newest `limit` commits reachable from `branch`.
    pub async fn commits(
        &self,
        repo: &RepoId,
        branch: &str,
        limit: u32,
    ) -> Result<Vec<CommitInfo>> {
        self.get_json(
            &format!(
                "/repos/{repo}/commits?sha={}&per_page={}",
                encode(branch),
                limit.clamp(1, 100)
            ),
            &format!("commits on `{}` in {repo}", display_text(branch)),
        )
        .await
    }

    /// Commits in `head` that are not in `base`, oldest first (first page only).
    pub async fn compare(&self, repo: &RepoId, base: &str, head: &str) -> Result<Comparison> {
        self.get_json(
            &format!(
                "/repos/{repo}/compare/{}...{}?per_page=100",
                encode(base),
                encode(head)
            ),
            &format!(
                "comparison {}...{} in {repo}",
                display_text(base),
                display_text(head)
            ),
        )
        .await
    }

    pub async fn release_by_tag(&self, repo: &RepoId, tag: &str) -> Result<Release> {
        self.get_json(
            &format!("/repos/{repo}/releases/tags/{}", encode(tag)),
            &format!("release `{}` of {repo}", display_text(tag)),
        )
        .await
    }

    /// Files and directories at the repository root (default branch).
    pub async fn root_entries(&self, repo: &RepoId) -> Result<Vec<ContentEntry>> {
        self.get_json(
            &format!("/repos/{repo}/contents"),
            &format!("contents of {repo}"),
        )
        .await
    }

    /// Raw text of a file on the default branch, `None` if it does not exist.
    ///
    /// Bounded to 1 MiB; prefer [`GitHub::file_text_bounded`] when the caller can
    /// pick a smaller limit.
    pub async fn file_text(&self, repo: &RepoId, path: &str) -> Result<Option<String>> {
        match self
            .file_text_bounded(repo, path, MAX_FILE_TEXT_BYTES)
            .await?
        {
            BoundedFileText::Text(text) => Ok(Some(text)),
            BoundedFileText::Missing => Ok(None),
            BoundedFileText::TooLarge => Err(Error::Parse {
                what: format!("{path} in {repo}"),
                message: format!("file exceeds the {MAX_FILE_TEXT_BYTES}-byte limit"),
            }),
        }
    }
}

/// Percent-encodes everything except unreserved characters and `/` (refs may contain it).
fn encode(value: &str) -> String {
    value
        .bytes()
        .fold(String::with_capacity(value.len()), |mut out, byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
                out.push(char::from(byte));
            } else {
                let _ = write!(out, "%{byte:02X}");
            }
            out
        })
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

#[cfg(test)]
#[path = "github/tests.rs"]
mod tests;
