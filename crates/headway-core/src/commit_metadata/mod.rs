//! Lazy commit-row annotations, separate from install/update candidates.
mod cache;
mod ci;
mod repository;
mod version;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Error, Headway, Result};
use cache::{CachedMetadata, REPOSITORY_TTL, Timed};
use repository::RepositorySnapshot;

pub(crate) use cache::Cache;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitCiState {
    Success,
    Failure,
    Pending,
    NotRun,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitVersionSource {
    Tag,
    Manifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMetadata {
    pub sha: String,
    pub version: Option<String>,
    #[serde(default)]
    pub version_source: Option<CommitVersionSource>,
    pub tags: Vec<String>,
    pub is_release: Option<bool>,
    pub release_url: Option<String>,
    pub ci_state: CommitCiState,
    pub ci_url: Option<String>,
    pub checks_count: u32,
    pub checked_at: DateTime<Utc>,
    pub notes: Vec<String>,
}

impl Headway {
    /// Lazily annotates an exact GitHub commit without changing update candidates.
    /// `refresh` bypasses both the 30-second row and five-minute repository caches.
    ///
    /// # Errors
    /// Rejects non-GitHub sources and non-hex SHAs outside 7..=40 characters.
    /// Abbreviated-SHA resolution errors propagate; optional CI/tag/release errors
    /// are represented as unknown metadata with notes.
    pub async fn commit_metadata(
        &self,
        id: &str,
        sha: &str,
        refresh: bool,
    ) -> Result<CommitMetadata> {
        if !(7..=40).contains(&sha.len()) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::InvalidInput(
                "Commit metadata requires a 7..40 character hexadecimal SHA.".into(),
            ));
        }
        let app = self.store().config()?.app(id)?.clone();
        let repo = app.repo_id()?;
        let sha = sha.to_ascii_lowercase();
        let repository = self
            .commit_metadata_cache
            .repository(&repo.full_name().to_ascii_lowercase())
            .await;
        let row = repository.row(&sha).await;
        let mut cached = row.lock().await;
        if !refresh
            && let Some(entry) = cached.as_ref().filter(|entry| entry.fresh())
            && repository
                .snapshot
                .lock()
                .await
                .as_ref()
                .is_some_and(|snapshot| snapshot.fresh(REPOSITORY_TTL))
        {
            return Ok(entry.value.clone());
        }
        let gh = self.github().await?;
        let full_sha = if sha.len() == 40 {
            sha.clone()
        } else {
            gh.commit(&repo, &sha).await?.sha.to_ascii_lowercase()
        };
        if full_sha.len() != 40
            || !full_sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !full_sha.starts_with(&sha)
        {
            return Err(Error::InvalidInput(
                "GitHub returned an invalid full commit SHA.".into(),
            ));
        }
        let snapshot = {
            let mut snapshot = repository.snapshot.lock().await;
            if refresh
                || !snapshot
                    .as_ref()
                    .is_some_and(|entry| entry.fresh(REPOSITORY_TTL))
            {
                *snapshot = Some(Timed::new(RepositorySnapshot::fetch(&gh, &repo).await));
            }
            snapshot
                .as_ref()
                .map(|entry| (entry.value.clone(), entry.at))
        };
        let mut metadata = CommitMetadata {
            sha: full_sha,
            version: None,
            version_source: None,
            tags: Vec::new(),
            is_release: None,
            release_url: None,
            ci_state: CommitCiState::Unknown,
            ci_url: None,
            checks_count: 0,
            checked_at: Utc::now(),
            notes: Vec::new(),
        };
        if let Some((snapshot, _)) = &snapshot {
            snapshot.apply(&mut metadata);
        }
        if metadata.version.is_none() {
            let version = repository.manifest_version(&gh, &repo, &metadata.sha).await;
            metadata.version_source = version
                .version
                .as_ref()
                .map(|_| CommitVersionSource::Manifest);
            metadata.version = version.version;
            metadata.notes.extend(version.notes);
        }
        ci::populate(&gh, &repo, &mut metadata).await;
        metadata.checked_at = Utc::now();
        if let Some((_, repository_at)) = snapshot {
            *cached = Some(CachedMetadata {
                value: metadata.clone(),
                at: std::time::Instant::now(),
                repository_at,
            });
        }
        Ok(metadata)
    }
}

#[cfg(test)]
mod tests;
