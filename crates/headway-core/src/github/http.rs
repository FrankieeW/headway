//! HTTP plumbing for the GitHub client: base-URL policy, bounded bodies, bounded
//! retries and redacted error text.
//!
//! Every logical API call has a total deadline and every response body is read
//! through an explicit byte cap, so a slow or hostile endpoint cannot make Headway
//! wait forever, grow memory without bound, or echo a queried secret into an error.

use std::time::Duration;

use chrono::Utc;
use reqwest::header::{ACCEPT, HeaderMap, RETRY_AFTER};
use reqwest::{RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use tokio::time::Instant;

use super::GitHub;
use super::models::ApiMessage;
use super::redact::{display_text, network, scrub_urls};
use crate::error::{Error, Result};

/// Default public GitHub REST endpoint.
pub const DEFAULT_API: &str = "https://api.github.com";

pub(super) const API_VERSION: &str = "2022-11-28";
pub(super) const JSON: &str = "application/vnd.github+json";

/// Total budget for one API call, including retries and reading its body.
pub(super) const API_DEADLINE: Duration = Duration::from_secs(60);
/// Total budget for one streaming asset download.
pub(super) const ASSET_DEADLINE: Duration = Duration::from_secs(30 * 60);
/// Connect timeout for the shared client.
pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Stall timeout between reads of a response.
pub(super) const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// At most three attempts per call: the original request plus two retries.
const MAX_ATTEMPTS: u32 = 3;
/// A retry never sleeps longer than this, and a server hint above it fails fast instead.
const RETRY_MAX_WAIT: Duration = Duration::from_secs(15);
/// First backoff step; doubles per attempt.
const RETRY_BASE_WAIT: Duration = Duration::from_millis(150);

/// Largest API JSON body Headway will buffer.
pub(super) const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
/// Largest error body Headway will read for a message.
pub(super) const MAX_ERROR_BYTES: usize = 8 * 1024;
/// Largest release asset Headway will download (GitHub itself caps assets at 2 GiB).
pub(super) const MAX_ASSET_BYTES: u64 = 4 * 1024 * 1024 * 1024;

impl GitHub {
    pub(super) fn request(&self, path: &str, accept: &str) -> RequestBuilder {
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

    /// Sends a request, retrying bounded transient failures, and classifies an
    /// unsuccessful final status into a stable [`Error`].
    pub(super) async fn send(
        &self,
        make: impl Fn() -> RequestBuilder,
        what: &str,
    ) -> Result<Response> {
        let response = self.send_raw(make).await?;
        if response.status().is_success() {
            return Ok(response);
        }
        Err(self.error_for(response, what).await)
    }

    /// Sends a request with bounded retries for 429, secondary rate limits and
    /// selected 5xx statuses. The final response is returned whatever its status.
    ///
    /// This bounds the wait *between* attempts, not a hung response: every caller must
    /// run it inside [`deadline`]. All public entry points (`get_json*`, the bounded
    /// file reads, pagination pages, commit pages and asset downloads) do.
    pub(super) async fn send_raw(&self, make: impl Fn() -> RequestBuilder) -> Result<Response> {
        let deadline = Instant::now() + API_DEADLINE;
        let mut attempt = 1_u32;
        loop {
            let response = make().send().await.map_err(network)?;
            let Some(wait) = retry_wait(&response, attempt, deadline) else {
                return Ok(response);
            };
            drop(response);
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    /// Reads a bounded error body and maps the status like before.
    pub(super) async fn error_for(&self, response: Response, what: &str) -> Error {
        let status = response.status();
        let headers = response.headers().clone();
        let body = read_prefix(response, MAX_ERROR_BYTES)
            .await
            .unwrap_or_default();
        classify(
            status,
            &headers,
            what,
            error_message(&body),
            self.token.is_some(),
        )
    }

    pub(super) async fn get_json<T: DeserializeOwned>(&self, path: &str, what: &str) -> Result<T> {
        self.get_json_with_limit(path, JSON, what, MAX_JSON_BYTES)
            .await
    }

    /// `get_json` with an explicit body cap for tests and small optional documents.
    pub(super) async fn get_json_with_limit<T: DeserializeOwned>(
        &self,
        path: &str,
        accept: &str,
        what: &str,
        limit: usize,
    ) -> Result<T> {
        let label = format!("GitHub request for {what}");
        let read = async {
            let response = self.send(|| self.request(path, accept), what).await?;
            let bytes = bounded_body(response, limit, what).await?;
            serde_json::from_slice(&bytes)
                .map_err(|err| Error::parse(format!("GitHub response for {what}"), &err))
        };
        deadline(API_DEADLINE, &label, read).await
    }
}

/// Reads an API body, rejecting one that declares or streams more than `limit` bytes.
pub(super) async fn bounded_body(response: Response, limit: usize, what: &str) -> Result<Vec<u8>> {
    limited_body(response, limit)
        .await?
        .ok_or_else(|| too_large(what, limit))
}

/// Reads a body within `limit`, returning `None` as soon as the limit is exceeded.
pub(super) async fn limited_body(mut response: Response, limit: usize) -> Result<Option<Vec<u8>>> {
    if let Some(length) = response.content_length()
        && length > u64::try_from(limit).unwrap_or(u64::MAX)
    {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}

/// Reads at most `limit` leading bytes, used for error messages.
pub(super) async fn read_prefix(mut response: Response, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    while bytes.len() < limit {
        let Some(chunk) = response.chunk().await.map_err(network)? else {
            break;
        };
        let take = chunk.len().min(limit - bytes.len());
        bytes.extend_from_slice(&chunk[..take]);
    }
    Ok(bytes)
}

/// Resolves a URL to the API base policy: HTTPS, or loopback HTTP for test servers.
pub(super) fn validate_api_base(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let shown = display_text(trimmed);
    let invalid = |detail: &str| Error::InvalidInput(format!("GitHub API URL `{shown}` {detail}"));
    if trimmed.is_empty() {
        return Err(invalid("is empty"));
    }
    let url = reqwest::Url::parse(trimmed).map_err(|_| invalid("must be an absolute URL"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("must not embed credentials"));
    }
    if url.query().is_some() {
        return Err(invalid("must not include a query string"));
    }
    if url.fragment().is_some() {
        return Err(invalid("must not include a fragment"));
    }
    match url.scheme() {
        "https" => {}
        "http" if loopback_host(&url) => {}
        "http" => {
            return Err(invalid(
                "must use HTTPS (plain HTTP is accepted only for loopback test servers)",
            ));
        }
        _ => return Err(invalid("must use an https:// URL")),
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Loopback hosts may use plain HTTP, for wiremock and local test servers only.
fn loopback_host(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Blocks for the response's hinted delay; `None` means "do not retry".
fn retry_wait(response: &Response, attempt: u32, deadline: Instant) -> Option<Duration> {
    if attempt >= MAX_ATTEMPTS {
        return None;
    }
    let status = response.status();
    let headers = response.headers();
    let retry_after = retry_after(headers);
    let exhausted = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        == Some("0");
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && (retry_after.is_some() || exhausted));
    let server_error = matches!(status.as_u16(), 500 | 502 | 503 | 504);
    if !(rate_limited || server_error) {
        return None;
    }
    let requested = match retry_after {
        Some(wait) => wait,
        None if exhausted => rate_limit_reset_wait(headers)?,
        None => backoff(attempt),
    };
    if requested > RETRY_MAX_WAIT {
        return None;
    }
    let now = Instant::now();
    if now >= deadline {
        return None;
    }
    let remaining = deadline - now;
    (requested <= remaining).then_some(requested)
}

fn backoff(attempt: u32) -> Duration {
    let factor = 1_u32
        .checked_shl(attempt.saturating_sub(1))
        .unwrap_or(u32::MAX);
    RETRY_BASE_WAIT.saturating_mul(factor).min(RETRY_MAX_WAIT)
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let seconds = (at.with_timezone(&Utc) - Utc::now()).num_seconds();
    if seconds <= 0 {
        return Some(Duration::ZERO);
    }
    u64::try_from(seconds).ok().map(Duration::from_secs)
}

fn rate_limit_reset_wait(headers: &HeaderMap) -> Option<Duration> {
    let epoch = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .parse::<i64>()
        .ok()?;
    let seconds = epoch - Utc::now().timestamp();
    if seconds <= 0 {
        return Some(Duration::ZERO);
    }
    let seconds = u64::try_from(seconds).ok()?;
    (seconds <= RETRY_MAX_WAIT.as_secs()).then_some(Duration::from_secs(seconds))
}

fn too_large(what: &str, limit: usize) -> Error {
    Error::Parse {
        what: format!("GitHub response for {what}"),
        message: format!("response exceeds the {limit}-byte limit"),
    }
}

fn error_message(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let message = serde_json::from_str::<ApiMessage>(text.as_ref()).map_or_else(
        |_| text.chars().take(200).collect::<String>(),
        |parsed| parsed.message,
    );
    let mut message = scrub_urls(&message);
    if message.chars().count() > 300 {
        message = message.chars().take(300).collect();
        message.push('…');
    }
    message
}

/// Runs `future` with a wall-clock budget, reporting timeouts as network failures.
pub(super) async fn deadline<F, T>(budget: Duration, what: &str, future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    match tokio::time::timeout(budget, future).await {
        Ok(result) => result,
        Err(_) => Err(Error::Network(format!(
            "{what} timed out after {} seconds",
            budget.as_secs()
        ))),
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
        .and_then(|epoch| chrono::DateTime::<Utc>::from_timestamp(epoch, 0))
        .map_or_else(|| "an unknown time".to_owned(), |at| at.to_rfc3339())
}
