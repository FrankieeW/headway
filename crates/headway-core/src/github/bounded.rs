//! Optional repository text with a hard accumulation limit, including error bodies.
//!
//! The response is dropped immediately on overflow; no complete body is buffered,
//! and the whole read runs under the shared API deadline.

use reqwest::StatusCode;

use super::http::{API_DEADLINE, deadline, limited_body};
use super::redact::display_text;
use super::{GitHub, encode};
use crate::error::{Error, Result};
use crate::repo::RepoId;

const RAW: &str = "application/vnd.github.raw+json";
const ERROR_TEXT: &str = "optional GitHub file UTF-8";

/// Outcome of a bounded optional repository file read.
#[derive(Debug, PartialEq, Eq)]
pub enum BoundedFileText {
    Missing,
    Text(String),
    TooLarge,
}

impl GitHub {
    /// Reads an optional file, rejecting declared or streamed bodies above `max_bytes`.
    /// The response is dropped immediately on overflow; no complete body is buffered.
    /// Error bodies are also bounded. Text must be UTF-8.
    ///
    /// # Errors
    /// Preserves GitHub authentication/rate-limit errors and transport failures;
    /// invalid UTF-8 produces a parse error. Missing files return `Missing`.
    pub async fn file_text_bounded(
        &self,
        repo: &RepoId,
        path: &str,
        max_bytes: usize,
    ) -> Result<BoundedFileText> {
        self.file_text_with_ref_bounded(repo, path, None, max_bytes)
            .await
    }

    /// Reads bounded file text at an explicit Git reference, never the default branch.
    pub async fn file_text_at_bounded(
        &self,
        repo: &RepoId,
        path: &str,
        reference: &str,
        max_bytes: usize,
    ) -> Result<BoundedFileText> {
        self.file_text_with_ref_bounded(repo, path, Some(reference), max_bytes)
            .await
    }

    async fn file_text_with_ref_bounded(
        &self,
        repo: &RepoId,
        path: &str,
        reference: Option<&str>,
        max_bytes: usize,
    ) -> Result<BoundedFileText> {
        let mut endpoint = format!("/repos/{repo}/contents/{}", encode(path));
        if let Some(reference) = reference {
            endpoint.push_str("?ref=");
            endpoint.push_str(&encode(reference));
        }
        let what = format!("{} in {repo}", display_text(path));
        let label = format!("GitHub request for {what}");
        let read = async {
            let response = self.send_raw(|| self.request(&endpoint, RAW)).await?;
            let status = response.status();
            if status == StatusCode::NOT_FOUND {
                return Ok(BoundedFileText::Missing);
            }
            if status.is_success() {
                return match limited_body(response, max_bytes).await? {
                    Some(bytes) => String::from_utf8(bytes)
                        .map(BoundedFileText::Text)
                        .map_err(|error| Error::parse(ERROR_TEXT, &error)),
                    None => Ok(BoundedFileText::TooLarge),
                };
            }
            Err(self.error_for(response, &what).await)
        };
        deadline(API_DEADLINE, &label, read).await
    }
}

#[cfg(test)]
mod tests;
