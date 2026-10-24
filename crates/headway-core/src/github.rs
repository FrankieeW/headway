//! Minimal GitHub REST client for the endpoints Headway uses.

mod bounded;
mod commit_metadata;
mod commits_page;
pub use bounded::BoundedFileText;
pub use commit_metadata::{CheckRun, CheckRuns, CombinedStatus, CommitStatus};
pub use commits_page::CommitListPage;

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::header::{ACCEPT, HeaderMap};
use reqwest::{RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt as _;

use crate::error::{Error, Result};
use crate::repo::RepoId;

pub const DEFAULT_API: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const JSON: &str = "application/vnd.github+json";

#[derive(Clone)]
pub struct GitHub {
    http: reqwest::Client,
    api: String,
    token: Option<String>,
}

impl GitHub {
    pub fn new(api: &str, token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("headway/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|err| Error::Network(err.to_string()))?;
        Ok(Self {
            http,
            api: api.trim_end_matches('/').to_owned(),
            token,
        })
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

    pub async fn branches(&self, repo: &RepoId) -> Result<Vec<Branch>> {
        self.get_json(
            &format!("/repos/{repo}/branches?per_page=100"),
            &format!("branches of {repo}"),
        )
        .await
    }

    /// One commit by SHA, branch or tag.
    pub async fn commit(&self, repo: &RepoId, reference: &str) -> Result<CommitInfo> {
        self.get_json(
            &format!("/repos/{repo}/commits/{}", encode(reference)),
            &format!("commit `{reference}` in {repo}"),
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
            &format!("commits on `{branch}` in {repo}"),
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
            &format!("comparison {base}...{head} in {repo}"),
        )
        .await
    }

    pub async fn releases(&self, repo: &RepoId) -> Result<Vec<Release>> {
        self.get_json(
            &format!("/repos/{repo}/releases?per_page=100"),
            &format!("releases of {repo}"),
        )
        .await
    }

    pub async fn release_by_tag(&self, repo: &RepoId, tag: &str) -> Result<Release> {
        self.get_json(
            &format!("/repos/{repo}/releases/tags/{}", encode(tag)),
            &format!("release `{tag}` of {repo}"),
        )
        .await
    }

    pub async fn tags(&self, repo: &RepoId) -> Result<Vec<Tag>> {
        self.get_json(
            &format!("/repos/{repo}/tags?per_page=100"),
            &format!("tags of {repo}"),
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
    pub async fn file_text(&self, repo: &RepoId, path: &str) -> Result<Option<String>> {
        let request = self.request(
            &format!("/repos/{repo}/contents/{}", encode(path)),
            "application/vnd.github.raw+json",
        );
        match self.send(request, &format!("{path} in {repo}")).await {
            Ok(response) => response.text().await.map(Some).map_err(network),
            Err(Error::RemoteNotFound(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Streams a release asset to `dest`, reporting `(bytes_done, bytes_total)`.
    /// Returns the SHA-256 of the downloaded bytes as lowercase hex.
    pub async fn download_asset(
        &self,
        repo: &RepoId,
        asset: &Asset,
        dest: &Path,
        on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<String> {
        let request = self.request(
            &format!("/repos/{repo}/releases/assets/{}", asset.id),
            "application/octet-stream",
        );
        let mut response = self.send(request, &format!("asset {}", asset.name)).await?;
        let total = response
            .content_length()
            .or_else(|| (asset.size > 0).then_some(asset.size));
        let mut file = tokio::fs::File::create(dest)
            .await
            .map_err(|err| Error::io_at("create", dest, err))?;
        let mut hasher = Sha256::new();
        let mut done = 0_u64;
        while let Some(chunk) = response.chunk().await.map_err(network)? {
            file.write_all(&chunk)
                .await
                .map_err(|err| Error::io_at("write", dest, err))?;
            hasher.update(&chunk);
            done += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            on_progress(done, total);
        }
        file.flush()
            .await
            .map_err(|err| Error::io_at("write", dest, err))?;
        Ok(to_hex(&hasher.finalize()))
    }

    fn request(&self, path: &str, accept: &str) -> RequestBuilder {
        let request = self
            .http
            .get(format!("{}{path}", self.api))
            .header(ACCEPT, accept)
            .header("X-GitHub-Api-Version", API_VERSION);
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str, what: &str) -> Result<T> {
        let response = self.send(self.request(path, JSON), what).await?;
        response
            .json::<T>()
            .await
            .map_err(|err| Error::parse(format!("GitHub response for {what}"), &err))
    }

    async fn send(&self, request: RequestBuilder, what: &str) -> Result<Response> {
        let response = request.send().await.map_err(network)?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ApiMessage>(&body).map_or_else(
            |_| body.chars().take(200).collect(),
            |parsed| parsed.message,
        );
        Err(classify(
            status,
            &headers,
            what,
            message,
            self.token.is_some(),
        ))
    }
}

fn classify(
    status: StatusCode,
    headers: &HeaderMap,
    what: &str,
    message: String,
    authenticated: bool,
) -> Error {
    let exhausted = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        == Some("0");
    let mentions_limit = message.to_ascii_lowercase().contains("rate limit");
    match status.as_u16() {
        401 => Error::Auth(format!("{message} (HTTP 401)")),
        403 | 429 if exhausted || mentions_limit => Error::RateLimited {
            reset: reset_time(headers),
        },
        403 => Error::Auth(format!("{message} (HTTP 403)")),
        404 if authenticated => Error::RemoteNotFound(what.to_owned()),
        404 => Error::RemoteNotFound(format!(
            "{what} (no GitHub token is configured; private repositories need one)"
        )),
        code => Error::GitHub {
            status: code,
            message,
        },
    }
}

fn reset_time(headers: &HeaderMap) -> String {
    headers
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|epoch| DateTime::<Utc>::from_timestamp(epoch, 0))
        .map_or_else(|| "an unknown time".to_owned(), |at| at.to_rfc3339())
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(network)`
fn network(err: reqwest::Error) -> Error {
    Error::Network(err.to_string())
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
struct RateLimitResponse {
    rate: RateLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoInfo {
    pub full_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub default_branch: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub archived: bool,
    pub html_url: String,
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

#[derive(Deserialize)]
struct ApiMessage {
    message: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn repo() -> RepoId {
        RepoId::parse("octo/tool").unwrap()
    }

    fn client(server: &MockServer, token: Option<&str>) -> GitHub {
        GitHub::new(&server.uri(), token.map(str::to_owned)).unwrap()
    }

    #[tokio::test]
    async fn sends_token_and_api_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .and(header("authorization", "Bearer ghp_test"))
            .and(header("x-github-api-version", API_VERSION))
            .and(header("accept", JSON))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"login": "frankie", "name": null})),
            )
            .mount(&server)
            .await;
        let user = client(&server, Some("ghp_test")).user().await.unwrap();
        assert_eq!(user.login, "frankie");
        assert_eq!(user.name, None);
    }

    #[tokio::test]
    async fn maps_error_statuses() {
        let server = MockServer::start().await;
        Mock::given(path("/user"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/rate_limit"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1700000000")
                    .set_body_json(json!({"message": "API rate limit exceeded"})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/repos/octo/tool"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
            .mount(&server)
            .await;
        Mock::given(path("/repos/octo/tool/tags"))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream exploded"))
            .mount(&server)
            .await;

        let gh = client(&server, None);
        assert!(
            matches!(gh.user().await, Err(Error::Auth(msg)) if msg.contains("Bad credentials"))
        );
        assert!(matches!(
            gh.rate_limit().await,
            Err(Error::RateLimited { reset }) if reset.starts_with("2023-11-14")
        ));
        let missing = gh.repo(&repo()).await.unwrap_err();
        assert!(matches!(&missing, Error::RemoteNotFound(msg) if msg.contains("no GitHub token")));
        assert!(matches!(
            gh.tags(&repo()).await,
            Err(Error::GitHub { status: 500, message }) if message == "upstream exploded"
        ));
    }

    #[tokio::test]
    async fn parses_commits_and_comparisons() {
        let server = MockServer::start().await;
        let commit = |sha: &str, message: &str| {
            json!({
                "sha": sha,
                "html_url": format!("https://github.com/octo/tool/commit/{sha}"),
                "commit": {
                    "message": message,
                    "author": {"name": "Ada", "date": "2026-10-01T10:00:00Z"},
                    "committer": {"name": "GitHub", "date": "2026-10-01T10:05:00Z"}
                },
                "author": null
            })
        };
        Mock::given(path("/repos/octo/tool/compare/aaa...main"))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ahead", "ahead_by": 2, "behind_by": 0,
                "commits": [commit("bbb", "first\n\nbody"), commit("ccc", "second")]
            })))
            .mount(&server)
            .await;
        Mock::given(path("/repos/octo/tool/commits"))
            .and(query_param("sha", "feature/x+y"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([commit("ddd", "x")])))
            .mount(&server)
            .await;

        let gh = client(&server, None);
        let comparison = gh.compare(&repo(), "aaa", "main").await.unwrap();
        assert_eq!(comparison.status, "ahead");
        assert_eq!(comparison.ahead_by, 2);
        assert_eq!(comparison.commits[0].commit.message, "first\n\nbody");
        assert!(comparison.commits[0].author.is_none());

        let commits = gh.commits(&repo(), "feature/x+y", 5).await.unwrap();
        assert_eq!(commits[0].sha, "ddd");
    }

    #[tokio::test]
    async fn downloads_assets_with_digest_and_progress() {
        let server = MockServer::start().await;
        Mock::given(path("/repos/octo/tool/releases/assets/7"))
            .and(header("accept", "application/octet-stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("tool.tar.gz");
        let asset = Asset {
            id: 7,
            name: "tool.tar.gz".into(),
            size: 5,
            browser_download_url: None,
            digest: None,
        };
        let mut progress = Vec::new();
        let digest = client(&server, None)
            .download_asset(&repo(), &asset, &dest, &mut |done, total| {
                progress.push((done, total));
            })
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
        assert_eq!(
            digest,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(progress.last(), Some(&(5, Some(5))));
    }

    #[tokio::test]
    async fn missing_files_are_none() {
        let server = MockServer::start().await;
        Mock::given(path("/repos/octo/tool/contents/Cargo.toml"))
            .and(header("accept", "application/vnd.github.raw+json"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("[package]\nname = \"tool\"\n"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/repos/octo/tool/contents/go.mod"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
            .mount(&server)
            .await;
        let gh = client(&server, Some("t"));
        assert!(
            gh.file_text(&repo(), "Cargo.toml")
                .await
                .unwrap()
                .unwrap()
                .contains("tool")
        );
        assert_eq!(gh.file_text(&repo(), "go.mod").await.unwrap(), None);
    }

    #[test]
    fn encodes_refs() {
        assert_eq!(encode("release/1.0"), "release/1.0");
        assert_eq!(encode("a b#c"), "a%20b%23c");
        assert_eq!(to_hex(&[0, 15, 255]), "000fff");
    }
}
