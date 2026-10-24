use crate::github::{GitHub, Release, Tag};
use crate::repo::RepoId;
use crate::version::compare_tags;

use super::{CommitMetadata, CommitVersionSource};

#[derive(Clone)]
pub(super) struct RepositorySnapshot {
    tags: Vec<Tag>,
    releases: Vec<Release>,
    complete: bool,
    notes: Vec<String>,
}

impl RepositorySnapshot {
    pub async fn fetch(gh: &GitHub, repo: &RepoId) -> Self {
        let (tags, releases) = tokio::join!(gh.tags(repo), gh.releases(repo));
        let mut notes = Vec::new();
        let mut complete = true;
        let tags = match tags {
            Ok(tags) => tags,
            Err(error) => {
                complete = false;
                notes.push(format!("Tag lookup unavailable: {error}"));
                Vec::new()
            }
        };
        let releases = match releases {
            Ok(releases) => releases,
            Err(error) => {
                complete = false;
                notes.push(format!("Release lookup unavailable: {error}"));
                Vec::new()
            }
        };
        if tags.len() >= 100 || releases.len() >= 100 {
            complete = false;
            notes.push("Tag/release scan reached its 100-item limit; unmatched older releases are unknown.".into());
        }
        Self {
            tags: tags.into_iter().take(100).collect(),
            releases: releases
                .into_iter()
                .take(100)
                .filter(|release| !release.draft)
                .collect(),
            complete,
            notes,
        }
    }

    pub fn apply(&self, metadata: &mut CommitMetadata) {
        // GitHub's repository-tags endpoint exposes the peeled commit SHA, not the
        // annotated tag object's SHA. Release target_commitish is never consulted.
        metadata.tags = self
            .tags
            .iter()
            .filter(|tag| tag.commit.sha.eq_ignore_ascii_case(&metadata.sha))
            .map(|tag| tag.name.clone())
            .collect();
        metadata.tags.sort_by(|a, b| compare_tags(b, a));
        metadata.tags.dedup();
        metadata.version = metadata
            .tags
            .iter()
            .find(|tag| is_version_tag(tag))
            .cloned();
        metadata.version_source = metadata.version.as_ref().map(|_| CommitVersionSource::Tag);
        let release = metadata.tags.iter().find_map(|tag| {
            self.releases
                .iter()
                .find(|release| release.tag_name == *tag)
        });
        metadata.is_release = release
            .map(|_| true)
            .or_else(|| self.complete.then_some(false));
        metadata.release_url = release.and_then(|release| release.html_url.clone());
        metadata.notes.extend(self.notes.clone());
    }
}

fn is_version_tag(tag: &str) -> bool {
    let version = tag.rsplit_once("-v").map_or(tag, |(_, version)| version);
    let version = version.strip_prefix('v').unwrap_or(version);
    super::version::is_version(version)
}
