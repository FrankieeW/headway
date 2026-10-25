//! Lazy commit browsing independent of installed/update state.
use futures_util::{StreamExt as _, stream};
use regex::Regex;

use crate::Headway;
use crate::asset::{self, Arch};
use crate::check::{commit_candidate, resolve_branch};
use crate::commit_metadata::{CommitCiState, ci};
use crate::config::{Install, Track};
use crate::error::{Error, Result};
use crate::github::GitHub;
use crate::repo::RepoId;
use crate::revision_history::{CommitEntry, CommitFilter, CommitPage, is_full_sha, validate_page};

/// CI lookups run in parallel, but never all at once.
const CI_CONCURRENCY: usize = 4;

impl Headway {
    /// Lists one page of the selected GitHub source's branch history.
    /// Without an anchor, page 1 resolves the branch's current tip. Reuse the
    /// returned anchor on all later pages to keep a stable history snapshot.
    ///
    /// # Errors
    /// Rejects non-GitHub sources, invalid pagination/anchors and unanchored
    /// later pages. GitHub repository, branch and page errors propagate.
    pub async fn list_commits(
        &self,
        id: &str,
        page: u32,
        per_page: u32,
        anchor: Option<&str>,
    ) -> Result<CommitPage> {
        self.list_commits_filtered(id, page, per_page, anchor, &CommitFilter::default())
            .await
    }

    /// Lists one page of history, narrowed by `filter`.
    ///
    /// Every commit of the page is annotated with the release it published (and
    /// whether that release has an asset for this machine) before the filter
    /// runs, so `--release` and `--tag` cost one repository scan, not one call
    /// per commit. `--ci` needs the check runs of every surviving commit and
    /// therefore costs two calls per commit.
    ///
    /// # Errors
    /// Rejects non-GitHub sources, invalid pagination/anchors, unanchored later
    /// pages and an invalid tag pattern. GitHub repository, branch and page
    /// errors propagate.
    pub async fn list_commits_filtered(
        &self,
        id: &str,
        page: u32,
        per_page: u32,
        anchor: Option<&str>,
        filter: &CommitFilter,
    ) -> Result<CommitPage> {
        validate_page(page, per_page)?;
        if anchor.is_some_and(|anchor| !is_full_sha(anchor)) {
            return Err(Error::InvalidInput(
                "Commit history anchor must be a full 40-character hexadecimal SHA.".into(),
            ));
        }
        if page > 1 && anchor.is_none() {
            return Err(Error::InvalidInput(
                "Later commit pages require the anchor returned by page 1.".into(),
            ));
        }
        let tag_filter = match &filter.tag {
            Some(pattern) => Some(Regex::new(pattern).map_err(|error| {
                Error::InvalidInput(format!("Invalid tag filter `{pattern}`: {error}"))
            })?),
            None => None,
        };
        let app = self.store().config()?.app(id)?.clone();
        let source = app.require_github("commit history")?;
        let repo = source.repo_id()?;
        let gh = self.github().await?;
        let configured_branch = if let Some(variant) = &source.options.variant {
            variant.branch.as_deref()
        } else {
            match &source.track {
                Track::Commit { branch } => branch.as_deref(),
                Track::Release { .. } | Track::Tag { .. } => None,
            }
        };
        let branch = resolve_branch(&gh, &repo, configured_branch).await?;
        let anchor = match anchor {
            Some(anchor) => anchor.to_ascii_lowercase(),
            None => gh.commit(&repo, &branch).await?.sha.to_ascii_lowercase(),
        };
        if !is_full_sha(&anchor) {
            return Err(Error::Parse {
                what: "GitHub branch tip".into(),
                message: "expected a full 40-character hexadecimal SHA".into(),
            });
        }
        let found = gh.commits_page(&repo, &anchor, page, per_page).await?;
        let next_page = if found.has_more {
            Some(page.checked_add(1).ok_or_else(|| {
                Error::InvalidInput("Commit page number cannot advance beyond u32::MAX.".into())
            })?)
        } else {
            None
        };
        let snapshot = if found.commits.is_empty() {
            crate::commit_metadata::repository::RepositorySnapshot::default()
        } else {
            self.repository_snapshot(&gh, &repo).await
        };
        let pattern = release_pattern(source);
        let mut commits: Vec<CommitEntry> = found
            .commits
            .iter()
            .map(|commit| {
                let candidate = commit_candidate(commit);
                let tags = snapshot.tags_for(&candidate.id);
                let release = snapshot.release_for(&candidate.id);
                let can_download = release.is_some_and(|release| {
                    asset::select(&release.assets, app.kind, pattern, Arch::host()).is_ok()
                });
                CommitEntry {
                    candidate,
                    tags,
                    release_tag: release.map(|release| release.tag_name.clone()),
                    release_url: release.and_then(|release| release.html_url.clone()),
                    can_download,
                    ci_state: None,
                }
            })
            .collect();
        let mut notes = snapshot.notes().to_vec();
        let listed = u32::try_from(commits.len()).unwrap_or(u32::MAX);
        if filter.releases_only {
            commits.retain(|commit| commit.release_tag.is_some());
        }
        if let Some(tag_filter) = &tag_filter {
            commits.retain(|commit| commit.tags.iter().any(|tag| tag_filter.is_match(tag)));
        }
        if let Some(wanted) = filter.ci {
            let (matched, warnings) = filter_ci(&gh, &repo, commits, wanted).await;
            commits = matched;
            notes.extend(warnings);
        }
        let remaining = u32::try_from(commits.len()).unwrap_or(u32::MAX);
        Ok(CommitPage {
            repo: repo.full_name(),
            branch,
            anchor,
            page,
            per_page,
            commits,
            has_more: found.has_more,
            next_page,
            filter: filter.clone(),
            filtered_out: listed.saturating_sub(remaining),
            notes,
        })
    }
}

#[cfg(test)]
#[path = "commits_tests.rs"]
mod tests;

/// The asset glob the app would use for a release download.
fn release_pattern(source: &crate::source::GithubSource) -> Option<&str> {
    match &source.install {
        Install::Asset(asset) => asset.pattern.as_deref(),
        Install::Source(_) => source
            .options
            .release_asset
            .as_ref()
            .and_then(|asset| asset.pattern.as_deref()),
    }
}

/// Retains matching commits in order, including warnings for filtered-out rows.
async fn filter_ci(
    gh: &GitHub,
    repo: &RepoId,
    commits: Vec<CommitEntry>,
    wanted: CommitCiState,
) -> (Vec<CommitEntry>, Vec<String>) {
    let mut results = stream::iter(commits)
        .map(|commit| async move {
            let summary = ci::summarize(gh, repo, &commit.candidate.id).await;
            (commit, summary)
        })
        .buffered(CI_CONCURRENCY);
    let mut kept = Vec::new();
    let mut notes = Vec::new();
    while let Some((commit, summary)) = results.next().await {
        notes.extend(
            summary
                .notes
                .into_iter()
                .map(|note| format!("{}: {note}", commit.candidate.id)),
        );
        if summary.state == wanted {
            kept.push(CommitEntry {
                ci_state: Some(summary.state),
                ..commit
            });
        }
    }
    (kept, notes)
}
