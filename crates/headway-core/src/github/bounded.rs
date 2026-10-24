//! Optional repository text with a hard accumulation limit, including error bodies.
use super::{ApiMessage, GitHub, classify, encode, network};
use crate::error::{Error, Result};
use crate::repo::RepoId;

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
        let request = self.request(&endpoint, "application/vnd.github.raw+json");
        let response = request.send().await.map_err(network)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(BoundedFileText::Missing);
        }
        if status.is_success() {
            return read_text(response, max_bytes).await;
        }
        let headers = response.headers().clone();
        let message = match read_text(response, max_bytes.min(8192)).await {
            Ok(BoundedFileText::Text(body)) => serde_json::from_str::<ApiMessage>(&body)
                .map_or_else(
                    |_| body.chars().take(200).collect(),
                    |parsed| parsed.message,
                ),
            Ok(BoundedFileText::TooLarge) => {
                "Optional file error response exceeds the byte limit".into()
            }
            Ok(BoundedFileText::Missing) => "Optional file is missing".into(),
            Err(Error::Parse { .. }) => "Optional file error response is not UTF-8".into(),
            Err(err) => return Err(err),
        };
        Err(classify(
            status,
            &headers,
            &format!("{path} in {repo}"),
            message,
            self.token.is_some(),
        ))
    }
}

async fn read_text(mut response: reqwest::Response, limit: usize) -> Result<BoundedFileText> {
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(limit).unwrap_or(u64::MAX))
    {
        return Ok(BoundedFileText::TooLarge);
    }
    // Capacity cannot grow past the limit: check every chunk before appending it.
    let mut bytes = Vec::with_capacity(limit);
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Ok(BoundedFileText::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes)
        .map(BoundedFileText::Text)
        .map_err(|error| Error::parse("optional GitHub file UTF-8", &error))
}

#[cfg(test)]
mod tests;
