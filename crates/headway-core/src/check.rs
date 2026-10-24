//! Asking GitHub what is newer than the installed version.

use std::cmp::Ordering;

use chrono::Utc;
use regex::Regex;

use crate::config::{Track, TrackedApp};
use crate::error::{Error, Result};
use crate::github::{CommitInfo, GitHub, Release, Tag};
use crate::repo::RepoId;
use crate::state::{Candidate, CheckResult, InstalledRef, UpdateStatus, ids_match};
use crate::version::Version;

/// Recent candidates listed when nothing is installed yet.
const RECENT: usize = 20;
const BODY_LIMIT: usize = 4000;

pub async fn check(
    gh: &GitHub,
    app: &TrackedApp,
    installed: Option<&InstalledRef>,
) -> Result<CheckResult> {
    let repo = app.repo_id()?;
    match &app.track {
        Track::Commit { branch } => check_commits(gh, &repo, branch.as_deref(), installed).await,
        Track::Release { prerelease, .. } => {
            let filter = app.track.tag_regex()?;
            let releases = gh.releases(&repo).await?;
            let candidates = release_candidates(&releases, *prerelease, filter.as_ref());
            Ok(evaluate_versions(candidates, installed))
        }
        Track::Tag { prerelease, .. } => {
            let filter = app.track.tag_regex()?;
            let tags = gh.tags(&repo).await?;
            let candidates = tag_candidates(&repo, &tags, *prerelease, filter.as_ref());
            Ok(evaluate_versions(candidates, installed))
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
fn newest_first(a: &Candidate, b: &Candidate) -> Ordering {
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
mod tests {
    use chrono::{DateTime, TimeZone as _};
    use serde_json::json;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::{AssetInstall, Install};
    use crate::testing::cli_app;

    fn installed(id: &str) -> InstalledRef {
        InstalledRef {
            id: id.into(),
            sha: Some(id.into()),
            version: None,
            installed_at: Utc::now(),
            path: None,
        }
    }

    fn commit_json(sha: &str, message: &str) -> serde_json::Value {
        json!({
            "sha": sha,
            "html_url": format!("https://github.com/owner/rg/commit/{sha}"),
            "commit": {
                "message": message,
                "author": {"name": "Ada", "date": "2026-10-01T10:00:00Z"},
                "committer": {"name": "GitHub", "date": "2026-10-02T10:00:00Z"}
            },
            "author": {"login": "ada"}
        })
    }

    fn release(tag: &str, prerelease: bool, day: u32) -> Release {
        Release {
            id: u64::from(day),
            tag_name: tag.into(),
            name: Some(format!("Release {tag}")),
            body: Some("notes".into()),
            draft: false,
            prerelease,
            published_at: Some(Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()),
            created_at: None,
            html_url: None,
            author: None,
            assets: vec![],
        }
    }

    async fn server_with_branch(head: &str) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(path("/repos/owner/rg"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "full_name": "owner/rg", "default_branch": "main", "html_url": "https://github.com/owner/rg"
            })))
            .mount(&server)
            .await;
        Mock::given(path("/repos/owner/rg/commits/main"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(commit_json(head, "head commit\n\ndetails")),
            )
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn commit_mode_lists_pending_newest_first() {
        let server = server_with_branch("ccc3333").await;
        Mock::given(path("/repos/owner/rg/compare/aaa1111...ccc3333"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ahead", "ahead_by": 2, "behind_by": 0,
                "commits": [commit_json("bbb2222", "second"), commit_json("ccc3333", "third")]
            })))
            .mount(&server)
            .await;
        let gh = GitHub::new(&server.uri(), None).unwrap();
        let result = check(&gh, &cli_app("rg"), Some(&installed("aaa1111")))
            .await
            .unwrap();
        assert_eq!(result.status, UpdateStatus::UpdateAvailable);
        assert_eq!(result.branch.as_deref(), Some("main"));
        let ids: Vec<_> = result.pending.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["ccc3333", "bbb2222"]);
        assert_eq!(result.next().unwrap().id, "bbb2222");
        assert_eq!(result.pending_total, 2);
        let latest = result.latest.unwrap();
        assert_eq!(latest.title, "head commit");
        assert_eq!(latest.body.as_deref(), Some("details"));
        assert_eq!(latest.author.as_deref(), Some("Ada"));
    }

    #[tokio::test]
    async fn commit_mode_up_to_date_and_not_installed() {
        let server = server_with_branch("ccc3333abcdef").await;
        Mock::given(path("/repos/owner/rg/commits"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                commit_json("ccc3333", "third"),
                commit_json("bbb2222", "second")
            ])))
            .mount(&server)
            .await;
        let gh = GitHub::new(&server.uri(), None).unwrap();
        let app = cli_app("rg");

        let fresh = check(&gh, &app, None).await.unwrap();
        assert_eq!(fresh.status, UpdateStatus::NotInstalled);
        assert_eq!(fresh.pending.len(), 2);

        // Abbreviated SHAs match the full head SHA.
        let current = check(&gh, &app, Some(&installed("ccc3333"))).await.unwrap();
        assert_eq!(current.status, UpdateStatus::UpToDate);
        assert!(current.pending.is_empty());
    }

    #[tokio::test]
    async fn commit_mode_detects_rewritten_history() {
        let server = server_with_branch("ccc3333").await;
        Mock::given(path("/repos/owner/rg/compare/dead000...ccc3333"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
            .mount(&server)
            .await;
        let gh = GitHub::new(&server.uri(), None).unwrap();
        let result = check(&gh, &cli_app("rg"), Some(&installed("dead000")))
            .await
            .unwrap();
        assert_eq!(result.status, UpdateStatus::Diverged);
    }

    #[test]
    fn releases_are_filtered_and_ordered_by_version() {
        let releases = vec![
            release("v1.9.0", false, 3),
            release("v1.10.0", false, 2),
            release("v2.0.0-rc.1", true, 4),
            Release {
                draft: true,
                ..release("v3.0.0", false, 5)
            },
            release("docs-2026", false, 6),
        ];
        let filter = Regex::new("^v").unwrap();
        let stable = release_candidates(&releases, false, Some(&filter));
        let result = evaluate_versions(
            stable,
            Some(&InstalledRef {
                sha: None,
                ..installed("v1.9.0")
            }),
        );
        assert_eq!(result.status, UpdateStatus::UpdateAvailable);
        assert_eq!(result.latest.unwrap().id, "v1.10.0");
        assert_eq!(
            result
                .pending
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["v1.10.0"]
        );

        let with_pre = release_candidates(&releases, true, Some(&filter));
        let result = evaluate_versions(with_pre, Some(&installed("v1.10.0")));
        assert_eq!(result.latest.unwrap().id, "v2.0.0-rc.1");
        assert!(result.pending[0].prerelease);
    }

    #[test]
    fn release_mode_edge_cases() {
        let candidates = release_candidates(
            &[release("v1.0.0", false, 1), release("v1.1.0", false, 2)],
            false,
            None,
        );
        let up_to_date = evaluate_versions(candidates.clone(), Some(&installed("v1.1.0")));
        assert_eq!(up_to_date.status, UpdateStatus::UpToDate);
        assert_eq!(up_to_date.pending_total, 0);

        // Installed tag was deleted upstream: compare versions instead.
        let deleted = evaluate_versions(candidates.clone(), Some(&installed("v1.0.5")));
        assert_eq!(
            deleted
                .pending
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["v1.1.0"]
        );
        let ahead = evaluate_versions(candidates.clone(), Some(&installed("v9.0.0")));
        assert_eq!(ahead.status, UpdateStatus::UpToDate);

        let none = evaluate_versions(candidates, None);
        assert_eq!(none.status, UpdateStatus::NotInstalled);
        assert_eq!(none.pending.len(), 2);

        let empty = evaluate_versions(Vec::new(), Some(&installed("v1.0.0")));
        assert_eq!(empty.status, UpdateStatus::UpToDate);
        assert!(empty.latest.is_none());
    }

    #[test]
    fn tags_skip_non_versions_and_prereleases() {
        let repo = RepoId::parse("owner/rg").unwrap();
        let tag = |name: &str| Tag {
            name: name.into(),
            commit: crate::github::TagCommit {
                sha: format!("sha-{name}"),
            },
        };
        let tags = [
            tag("v1.0.0"),
            tag("v1.1.0-beta.1"),
            tag("nightly"),
            tag("v0.9.0"),
        ];
        let stable = tag_candidates(&repo, &tags, false, None);
        assert_eq!(
            stable.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["v1.0.0", "v0.9.0"]
        );
        assert_eq!(stable[0].sha.as_deref(), Some("sha-v1.0.0"));
        assert_eq!(
            stable[0].url.as_deref(),
            Some("https://github.com/owner/rg/tree/v1.0.0")
        );
        assert_eq!(tag_candidates(&repo, &tags, true, None).len(), 3);
    }

    #[tokio::test]
    async fn release_mode_end_to_end() {
        let server = MockServer::start().await;
        Mock::given(path("/repos/owner/rg/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 2, "tag_name": "v1.1.0", "name": "", "draft": false, "prerelease": false,
                 "published_at": "2026-02-01T00:00:00Z", "body": "", "assets": []},
                {"id": 1, "tag_name": "v1.0.0", "name": "First", "draft": false, "prerelease": false,
                 "published_at": "2026-01-01T00:00:00Z", "assets": []}
            ])))
            .mount(&server)
            .await;
        let app = TrackedApp {
            track: Track::Release {
                prerelease: false,
                tag_filter: None,
            },
            install: Install::Asset(AssetInstall::default()),
            ..cli_app("rg")
        };
        let gh = GitHub::new(&server.uri(), None).unwrap();
        let result = check(&gh, &app, Some(&installed("v1.0.0"))).await.unwrap();
        let latest = result.latest.unwrap();
        assert_eq!(
            latest.title, "v1.1.0",
            "empty release names fall back to the tag"
        );
        assert_eq!(latest.body, None);
        assert_eq!(
            latest.date,
            Some(
                DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z")
                    .unwrap()
                    .to_utc()
            )
        );
    }

    #[test]
    fn mixed_tags_sort_without_panicking() {
        // v2.0.0 (day 1) > v1.9.5 (day 3) by version; v1.9.5 > `stable` (day 2) and
        // `stable` > v2.0.0 by date used to form a cycle, and `sort_by` panics on those.
        let mut releases = vec![
            release("v2.0.0", false, 1),
            release("v1.9.5", false, 3),
            release("stable", false, 2),
        ];
        releases.extend((4..28).map(|day| release(&format!("v0.{day}.0"), false, day)));
        let mut candidates = release_candidates(&releases, false, None);
        candidates.sort_by(newest_first);
        assert_eq!(candidates[0].id, "v2.0.0");
        assert_eq!(candidates[1].id, "v1.9.5");
        assert_eq!(
            candidates.last().unwrap().id,
            "stable",
            "non-version tags sort last"
        );
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "hé…");
        assert_eq!(truncate("hi", 5), "hi");
    }
}
