//! GitHub REST wire types. Deserialized from API responses, never executed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub limit: u32,
    pub remaining: u32,
    /// Unix time when the window resets.
    pub reset: i64,
}

#[derive(Deserialize)]
pub(super) struct RateLimitResponse {
    pub(super) rate: RateLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoInfo {
    pub full_name: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub default_branch: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub archived: bool,
    pub html_url: String,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub parent: Option<RelatedRepo>,
    #[serde(default)]
    pub source: Option<RelatedRepo>,
}

/// The identity and default branch supplied by GitHub for a fork's ancestry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedRepo {
    pub full_name: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    #[serde(default)]
    pub html_url: Option<String>,
    pub commit: CommitDetail,
    /// The GitHub account, when the author email maps to one.
    #[serde(default)]
    pub author: Option<Account>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitDetail {
    pub message: String,
    #[serde(default)]
    pub author: Option<GitActor>,
    #[serde(default)]
    pub committer: Option<GitActor>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitActor {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Comparison {
    /// `ahead`, `behind`, `diverged` or `identical`.
    pub status: String,
    #[serde(default)]
    pub ahead_by: u32,
    #[serde(default)]
    pub behind_by: u32,
    #[serde(default)]
    pub commits: Vec<CommitInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub id: u64,
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub author: Option<Account>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_download_url: Option<String>,
    /// `sha256:<hex>`, published by GitHub for release assets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub name: String,
    pub commit: TagCommit,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TagCommit {
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentEntry {
    pub name: String,
    /// `file`, `dir`, `symlink` or `submodule`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// Error body of a failed API response; only ever displayed, never trusted.
#[derive(Deserialize)]
pub(super) struct ApiMessage {
    pub(super) message: String,
}
