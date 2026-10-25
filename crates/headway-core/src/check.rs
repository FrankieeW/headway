//! Finding out what is newer than the installed version, for every kind of source.

use std::cmp::Ordering;

use chrono::Utc;
use regex::Regex;

use crate::config::{Track, TrackedApp};
use crate::error::{Error, Result};
use crate::github::{CommitInfo, GitHub, Release, Tag};
use crate::repo::RepoId;
use crate::shell::ShellEnv;
use crate::source::{GithubSource, Source};
use crate::state::{Candidate, CheckResult, InstalledRef, UpdateStatus, ids_match};
use crate::version::Version;

/// Recent candidates listed when nothing is installed yet.
pub(crate) const RECENT: usize = 20;
const BODY_LIMIT: usize = 4000;

/// What a check needs: GitHub for GitHub sources, the shell environment for the rest.
pub struct Probe<'a> {
    pub gh: &'a GitHub,
    pub env: &'a ShellEnv,
    pub cancel: &'a tokio_util::sync::CancellationToken,
}

/// The result of a check and what it learned about the installed version.
#[derive(Debug, Clone)]
pub struct Checked {
    pub result: CheckResult,
    pub installed: InstalledSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledSync {
    /// Headway's own record stands (GitHub sources: it built or downloaded the version).
    Keep,
    /// What the system reports (npm, Homebrew, scripts); `None` when not installed.
    Detected(Option<String>),
}

pub async fn check(
    probe: &Probe<'_>,
    app: &TrackedApp,
    installed: Option<&InstalledRef>,
) -> Result<Checked> {
    match &app.source {
        Source::Github(github) => Ok(Checked {
            result: check_github(probe.gh, github, installed).await?,
            installed: InstalledSync::Keep,
        }),
        Source::Npm(npm) => crate::npm::check(probe.cancel, probe.env, npm).await,
        Source::Brew(brew) => crate::brew::check(probe.cancel, probe.env, brew).await,
        Source::Script(script) => crate::script::check(probe.cancel, probe.env, script).await,
    }
}

pub async fn check_github(
    gh: &GitHub,
    github: &GithubSource,
    installed: Option<&InstalledRef>,
) -> Result<CheckResult> {
    let repo = github.repo_id()?;
    match &github.track {
        Track::Commit { branch } => check_commits(gh, &repo, branch.as_deref(), installed).await,
        Track::Release { prerelease, .. } => {
            let filter = github.track.tag_regex()?;
            // Releases come newest first, so a bounded list still contains the latest
            // release; truncation only drops older history from `pending`.
            let releases = gh.releases(&repo).await?;
            let candidates = release_candidates(&releases, *prerelease, filter.as_ref());
            Ok(evaluate_versions(candidates, installed))
        }
        Track::Tag { prerelease, .. } => {
            let filter = github.track.tag_regex()?;
            let tags = gh.tags_page(&repo).await?;
            let candidates = tag_candidates(&repo, &tags.items, *prerelease, filter.as_ref());
            let result = evaluate_versions(candidates, installed);
            // The tag API has no documented ordering, so a truncated scan cannot prove
            // which tag is latest. Report "unknown" instead of a silently wrong latest;
            // a tag filter narrows the scan for repositories with more than 500 tags.
            Ok(
                if tags.truncated && result.status != UpdateStatus::NotInstalled {
                    CheckResult {
                        status: UpdateStatus::Unknown,
                        latest: None,
                        pending: Vec::new(),
                        pending_total: 0,
                        ..result
                    }
                } else {
                    result
                },
            )
        }
    }
}

/// The configured branch, or the repository's default branch.
pub async fn resolve_branch(gh: &GitHub, repo: &RepoId, branch: Option<&str>) -> Result<String> {
    match branch {
        Some(branch) => Ok(branch.to_owned()),
        None => Ok(gh.repo(repo).await?.default_branch),
    }
}

async fn check_commits(
    gh: &GitHub,
    repo: &RepoId,
    branch: Option<&str>,
    installed: Option<&InstalledRef>,
) -> Result<CheckResult> {
    let branch = resolve_branch(gh, repo, branch).await?;
    let head = gh.commit(repo, &branch).await?;
    let latest = commit_candidate(&head);
    let result = |status, pending: Vec<Candidate>, total: u32| CheckResult {
        checked_at: Utc::now(),
        status,
        branch: Some(branch.clone()),
        latest: Some(latest.clone()),
        pending_total: total.max(u32::try_from(pending.len()).unwrap_or(u32::MAX)),
        pending,
    };

    let Some(installed_sha) = installed.and_then(|installed| installed.sha.as_deref()) else {
        let recent = gh
            .commits(repo, &branch, u32::try_from(RECENT).unwrap_or(20))
            .await?;
        return Ok(result(
            UpdateStatus::NotInstalled,
            recent.iter().map(commit_candidate).collect(),
            0,
        ));
    };
    if ids_match(installed_sha, &head.sha) {
        return Ok(result(UpdateStatus::UpToDate, Vec::new(), 0));
    }
    match gh.compare(repo, installed_sha, &head.sha).await {
        Ok(comparison) => {
            // GitHub lists compare commits oldest first; Headway shows newest first.
            let pending: Vec<Candidate> = comparison
                .commits
                .iter()
                .rev()
                .map(commit_candidate)
                .collect();
            let status = match comparison.status.as_str() {
                "identical" => UpdateStatus::UpToDate,
                "ahead" => UpdateStatus::UpdateAvailable,
                _ => UpdateStatus::Diverged,
            };
            let pending = if status == UpdateStatus::UpToDate {
                Vec::new()
            } else {
                pending
            };
            Ok(result(status, pending, comparison.ahead_by))
        }
        // The installed commit no longer exists upstream (rewritten history).
        Err(Error::RemoteNotFound(_)) => Ok(result(UpdateStatus::Diverged, Vec::new(), 0)),
        Err(err) => Err(err),
    }
}

pub fn commit_candidate(commit: &CommitInfo) -> Candidate {
    let message = commit.commit.message.trim();
    let (title, body) = message
        .split_once('\n')
        .map_or((message, ""), |(title, body)| (title, body));
    let author = commit
        .commit
        .author
        .as_ref()
        .and_then(|actor| actor.name.clone())
        .or_else(|| commit.author.as_ref().map(|account| account.login.clone()));
    let date = commit
        .commit
        .committer
        .as_ref()
        .and_then(|actor| actor.date)
        .or_else(|| commit.commit.author.as_ref().and_then(|actor| actor.date));
    Candidate {
        id: commit.sha.clone(),
        sha: Some(commit.sha.clone()),
        title: title.trim().to_owned(),
        body: non_empty(body.trim()).map(|body| truncate(body, BODY_LIMIT)),
        author,
        date,
        url: commit.html_url.clone(),
        prerelease: false,
    }
}

pub fn release_candidate(release: &Release) -> Candidate {
    let title = release
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&release.tag_name);
    Candidate {
        id: release.tag_name.clone(),
        sha: None,
        title: title.to_owned(),
        body: release
            .body
            .as_deref()
            .map(str::trim)
            .and_then(non_empty)
            .map(|body| truncate(body, BODY_LIMIT)),
        author: release.author.as_ref().map(|author| author.login.clone()),
        date: release.published_at.or(release.created_at),
        url: release.html_url.clone(),
        prerelease: release.prerelease,
    }
}

fn release_candidates(
    releases: &[Release],
    prerelease: bool,
    filter: Option<&Regex>,
) -> Vec<Candidate> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter(|release| prerelease || !release.prerelease)
        .filter(|release| filter.is_none_or(|re| re.is_match(&release.tag_name)))
        .map(release_candidate)
        .collect()
}

fn tag_candidates(
    repo: &RepoId,
    tags: &[Tag],
    prerelease: bool,
    filter: Option<&Regex>,
) -> Vec<Candidate> {
    tags.iter()
        .filter(|tag| filter.is_none_or(|re| re.is_match(&tag.name)))
        .filter_map(|tag| {
            let version = Version::parse(&tag.name)?;
            (prerelease || !version.is_prerelease()).then(|| Candidate {
                id: tag.name.clone(),
                sha: Some(tag.commit.sha.clone()),
                title: tag.name.clone(),
                body: None,
                author: None,
                date: None,
                url: Some(format!("{}/tree/{}", repo.html_url(), tag.name)),
                prerelease: version.is_prerelease(),
            })
        })
        .collect()
}

/// Newest first: by version, then by date for tags that don't parse as versions.
///
/// A total order (required by `sort_by`): every version-like tag comes before every other
/// tag; versions compare by version, then date, then id; the rest by date, then id.
pub(crate) fn newest_first(a: &Candidate, b: &Candidate) -> Ordering {
    let by_date_then_id = || b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id));
    match (Version::parse(&a.id), Version::parse(&b.id)) {
        (Some(x), Some(y)) => y.cmp(&x).then_with(by_date_then_id),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => by_date_then_id(),
    }
}

fn evaluate_versions(
    mut candidates: Vec<Candidate>,
    installed: Option<&InstalledRef>,
) -> CheckResult {
    candidates.sort_by(newest_first);
    let latest = candidates.first().cloned();
    let result = |status, pending: Vec<Candidate>| CheckResult {
        checked_at: Utc::now(),
        status,
        branch: None,
        latest: latest.clone(),
        pending_total: if status == UpdateStatus::NotInstalled {
            0
        } else {
            u32::try_from(pending.len()).unwrap_or(u32::MAX)
        },
        pending,
    };

    let Some(installed) = installed else {
        return result(
            UpdateStatus::NotInstalled,
            candidates.into_iter().take(RECENT).collect(),
        );
    };
    // A source commit outside the known version sequence is not a version number.
    // In particular, a hexadecimal SHA's leading digits must not imply "newer".
    if installed.id.len() == 40
        && installed.id.bytes().all(|byte| byte.is_ascii_hexdigit())
        && installed
            .sha
            .as_deref()
            .is_some_and(|sha| sha.eq_ignore_ascii_case(&installed.id))
        && !candidates
            .iter()
            .any(|candidate| candidate.id == installed.id)
    {
        return result(UpdateStatus::Unknown, Vec::new());
    }
    if latest
        .as_ref()
        .is_none_or(|latest| latest.id == installed.id)
    {
        return result(UpdateStatus::UpToDate, Vec::new());
    }
    let newer: Vec<Candidate> = match candidates
        .iter()
        .position(|candidate| candidate.id == installed.id)
    {
        Some(index) => candidates[..index].to_vec(),
        // Installed tag no longer listed (deleted, or filtered out): compare as versions.
        None => match Version::parse(&installed.id) {
            Some(current) => candidates
                .into_iter()
                .filter(|candidate| {
                    Version::parse(&candidate.id).is_some_and(|version| version > current)
                })
                .collect(),
            None => latest.iter().cloned().collect(),
        },
    };
    let status = if newer.is_empty() {
        UpdateStatus::UpToDate
    } else {
        UpdateStatus::UpdateAvailable
    };
    result(status, newer)
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
