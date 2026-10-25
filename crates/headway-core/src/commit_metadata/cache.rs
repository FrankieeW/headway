use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use super::{CommitMetadata, repository::RepositorySnapshot};

pub(super) const REPOSITORY_TTL: Duration = Duration::from_secs(300);
pub(super) const COMMIT_TTL: Duration = Duration::from_secs(30);

pub(super) struct Timed<T> {
    pub value: T,
    pub at: Instant,
}

impl<T> Timed<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            at: Instant::now(),
        }
    }

    pub fn fresh(&self, ttl: Duration) -> bool {
        self.at.elapsed() < ttl
    }
}

pub(super) struct CachedMetadata {
    pub value: CommitMetadata,
    pub at: Instant,
    pub repository_at: Instant,
}

impl CachedMetadata {
    pub fn fresh(&self) -> bool {
        self.at.elapsed() < COMMIT_TTL && self.repository_at.elapsed() < REPOSITORY_TTL
    }
}

type Row = Arc<Mutex<Option<CachedMetadata>>>;
type Version = Arc<Mutex<Option<super::version::ManifestLookup>>>;

#[derive(Default)]
pub(crate) struct Cache {
    repositories: Mutex<BTreeMap<String, Arc<RepositoryCache>>>,
}

#[derive(Default)]
pub(super) struct RepositoryCache {
    pub snapshot: Mutex<Option<Timed<RepositorySnapshot>>>,
    rows: Mutex<BTreeMap<String, Row>>,
    versions: Mutex<BTreeMap<String, Version>>,
}

impl Cache {
    pub(super) async fn repository(&self, repo: &str) -> Arc<RepositoryCache> {
        let mut repositories = self.repositories.lock().await;
        if !repositories.contains_key(repo) && repositories.len() >= 32 {
            repositories.clear();
        }
        Arc::clone(repositories.entry(repo.to_owned()).or_default())
    }
}

impl RepositoryCache {
    /// The cached tag and release snapshot, refreshed when it is stale.
    pub(crate) async fn snapshot(
        &self,
        gh: &crate::github::GitHub,
        repo: &crate::repo::RepoId,
        refresh: bool,
    ) -> super::repository::RepositorySnapshot {
        let mut guard = self.snapshot.lock().await;
        if refresh
            || !guard
                .as_ref()
                .is_some_and(|entry| entry.fresh(REPOSITORY_TTL))
        {
            *guard = Some(Timed::new(
                super::repository::RepositorySnapshot::fetch(gh, repo).await,
            ));
        }
        guard
            .as_ref()
            .map(|entry| entry.value.clone())
            .unwrap_or_default()
    }

    pub async fn manifest_version(
        &self,
        gh: &crate::github::GitHub,
        repo: &crate::repo::RepoId,
        sha: &str,
    ) -> super::version::ManifestLookup {
        let entry = {
            let mut versions = self.versions.lock().await;
            if !versions.contains_key(sha) && versions.len() >= 256 {
                versions.clear();
            }
            Arc::clone(versions.entry(sha.to_owned()).or_default())
        };
        let mut cached = entry.lock().await;
        if let Some(version) = cached.as_ref() {
            return version.clone();
        }
        let version = super::version::fetch(gh, repo, sha).await;
        if version.cacheable {
            *cached = Some(version.clone());
        }
        version
    }

    pub async fn row(&self, sha: &str) -> Row {
        let mut rows = self.rows.lock().await;
        if !rows.contains_key(sha) && rows.len() >= 256 {
            rows.clear();
        }
        Arc::clone(rows.entry(sha.to_owned()).or_default())
    }
}
