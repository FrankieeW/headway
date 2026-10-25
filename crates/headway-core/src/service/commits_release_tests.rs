//! Release-aware commit listing: every commit of a page is annotated with the
//! release it published, and the page can be narrowed by release, tag or CI.
use serde_json::{Value, json};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::Headway;
use crate::commit_metadata::CommitCiState;
use crate::config::Track;
use crate::paths::Paths;
use crate::revision_history::CommitFilter;
use crate::shell::ShellEnv;
use crate::testing::{cli_app, with_github};
use crate::token::MemoryStore;

const ANCHOR: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RELEASED: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PLAIN: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn headway(server: &MockServer) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .secrets(std::sync::Arc::new(MemoryStore::with_token(
            "fixture-token",
        )))
        .env(ShellEnv::default())
        .api(server.uri())
        .build()
        .unwrap();
    hw.add(with_github(cli_app("selected-app"), |source| {
        crate::GithubSource {
            repo: "acme/project".into(),
            track: Track::Commit {
                branch: Some("main".into()),
            },
            ..source
        }
    }))
    .unwrap();
    (dir, hw)
}

fn commit(sha: &str, title: &str) -> Value {
    json!({"sha":sha,"html_url":format!("https://github.com/acme/project/commit/{sha}"),
        "commit":{"message":title,
            "author":{"name":"Builder","date":"2026-10-01T00:00:00Z"},
            "committer":{"date":"2026-10-02T00:00:00Z"}}})
}

/// The history page, the tag list and the release list of one repository.
async fn repository(server: &MockServer, with_release: bool) {
    Mock::given(path("/repos/acme/project/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit(ANCHOR, "Tip")))
        .mount(server)
        .await;
    Mock::given(path("/repos/acme/project/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            commit(RELEASED, "Published"),
            commit(PLAIN, "Plain")
        ])))
        .mount(server)
        .await;
    let mut tags = Vec::new();
    let mut releases = Vec::new();
    if with_release {
        tags.push(json!({"name":"v1.2.3","commit":{"sha":RELEASED}}));
        releases.push(
            json!({"id":7,"tag_name":"v1.2.3","draft":false,"prerelease":false,
            "html_url":"https://github.com/acme/project/releases/tag/v1.2.3",
            "assets":[{"id":9,"name":"tool-v1.2.3-x86_64-apple-darwin.tar.gz","size":10}]}),
        );
    }
    Mock::given(path("/repos/acme/project/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(tags)))
        .mount(server)
        .await;
    Mock::given(path("/repos/acme/project/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(releases)))
        .mount(server)
        .await;
}

/// The CI checks of one commit.
async fn ci(server: &MockServer, sha: &str, conclusion: &str) {
    Mock::given(path(format!(
        "/repos/acme/project/commits/{sha}/check-runs"
    )))
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "total_count":1,
        "check_runs":[{"name":"build","status":"completed","conclusion":conclusion}]
    })))
    .mount(server)
    .await;
    Mock::given(path(format!("/repos/acme/project/commits/{sha}/status")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state":"","total_count":0,"statuses":[]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn every_commit_carries_the_release_it_published() {
    // Given a repository whose second commit published a release.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the history is listed without a filter.
    let found = hw.list_commits("selected-app", 1, 20, None).await.unwrap();
    // Then both commits are listed and only the released one names its release.
    assert_eq!(found.commits.len(), 2);
    assert_eq!(found.commits[0].candidate.id, RELEASED);
    assert_eq!(found.commits[0].release_tag.as_deref(), Some("v1.2.3"));
    assert_eq!(
        found.commits[0].release_url.as_deref(),
        Some("https://github.com/acme/project/releases/tag/v1.2.3")
    );
    assert_eq!(found.commits[0].tags, ["v1.2.3"]);
    assert_eq!(found.commits[1].release_tag, None);
    assert!(found.commits[1].tags.is_empty());
    assert_eq!(found.filtered_out, 0);
}

#[tokio::test]
async fn the_release_filter_keeps_only_commits_with_a_release() {
    // Given the same repository.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the history is narrowed to published releases.
    let filter = CommitFilter {
        releases_only: true,
        ..CommitFilter::default()
    };
    let found = hw
        .list_commits_filtered("selected-app", 1, 20, None, &filter)
        .await
        .unwrap();
    // Then only the released commit survives and the page reports the rest.
    assert_eq!(found.commits.len(), 1);
    assert_eq!(found.commits[0].candidate.id, RELEASED);
    assert_eq!(found.filtered_out, 1);
    assert!(found.filter.releases_only);
}

#[tokio::test]
async fn the_tag_filter_matches_the_tags_of_a_commit() {
    // Given a repository with one version tag.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the history is narrowed by a tag pattern.
    let filter = CommitFilter {
        tag: Some("^v1\\.".into()),
        ..CommitFilter::default()
    };
    let found = hw
        .list_commits_filtered("selected-app", 1, 20, None, &filter)
        .await
        .unwrap();
    // Then the tagged commit is kept and the untagged one is not.
    assert_eq!(found.commits.len(), 1);
    assert_eq!(found.commits[0].candidate.id, RELEASED);
    assert_eq!(found.filtered_out, 1);
    // And a pattern that matches nothing empties the page.
    let none = CommitFilter {
        tag: Some("^v9".into()),
        ..CommitFilter::default()
    };
    let empty = hw
        .list_commits_filtered("selected-app", 1, 20, None, &none)
        .await
        .unwrap();
    assert!(empty.commits.is_empty());
    assert_eq!(empty.filtered_out, 2);
}

#[tokio::test]
async fn an_invalid_tag_pattern_is_rejected_before_any_request() {
    // Given a repository without any fixture.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    let filter = CommitFilter {
        tag: Some("v1.(".into()),
        ..CommitFilter::default()
    };
    // When the history is narrowed by a broken pattern.
    let failed = hw
        .list_commits_filtered("selected-app", 1, 20, None, &filter)
        .await;
    // Then the request is rejected locally.
    assert!(matches!(failed, Err(crate::Error::InvalidInput(_))));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_ci_filter_keeps_only_commits_with_that_ci_state() {
    // Given a repository where one commit passed CI.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    ci(&server, RELEASED, "success").await;
    ci(&server, PLAIN, "failure").await;
    // When the history is narrowed to commits whose CI passed.
    let filter = CommitFilter {
        ci: Some(CommitCiState::Success),
        ..CommitFilter::default()
    };
    let found = hw
        .list_commits_filtered("selected-app", 1, 20, None, &filter)
        .await
        .unwrap();
    // Then only the passing commit survives and it carries its CI state.
    assert_eq!(found.commits.len(), 1);
    assert_eq!(found.commits[0].candidate.id, RELEASED);
    assert_eq!(found.commits[0].ci_state, Some(CommitCiState::Success));
    assert_eq!(found.filtered_out, 1);
}

#[tokio::test]
async fn an_unfiltered_listing_never_reads_the_ci_state() {
    // Given a repository whose CI endpoints would fail loudly if they were read.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the history is listed without a filter.
    let found = hw.list_commits("selected-app", 1, 20, None).await.unwrap();
    // Then no CI state is claimed for any commit.
    assert!(found.commits.iter().all(|commit| commit.ci_state.is_none()));
    let requests: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request.url.to_string())
        .collect();
    assert!(
        !requests
            .iter()
            .any(|url| url.contains("check-runs") || url.ends_with("status")),
        "{requests:?}"
    );
}

#[tokio::test]
async fn unavailable_release_metadata_is_reported_in_filtered_pages() {
    // Given readable history but a forbidden release endpoint.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    Mock::given(path("/repos/acme/project/releases"))
        .respond_with(ResponseTemplate::new(403))
        .with_priority(1)
        .mount(&server)
        .await;
    let filter = CommitFilter {
        releases_only: true,
        ..Default::default()
    };

    // When filtering by release with incomplete metadata.
    let page = hw
        .list_commits_filtered("selected-app", 1, 20, None, &filter)
        .await
        .unwrap();

    // Then an empty filtered page explicitly carries the lookup failure.
    assert!(page.commits.is_empty());
    assert_eq!(page.notes.len(), 1);
}

#[tokio::test]
async fn empty_filtered_pages_keep_the_original_cursor() {
    // Given a full history page with no matching release and more history available.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, false).await;
    Mock::given(path("/repos/acme/project/commits"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([commit(PLAIN, "Plain")]))
                .insert_header(
                    "link",
                    "<https://api.github.com/commits?page=2>; rel=\"next\"",
                ),
        )
        .with_priority(1)
        .mount(&server)
        .await;

    // When all commits on this page are filtered out.
    let page = hw
        .list_commits_filtered(
            "selected-app",
            1,
            20,
            None,
            &CommitFilter {
                releases_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Then callers can still reach older pages using the immutable anchor.
    assert!(page.commits.is_empty());
    assert_eq!(page.filtered_out, 1);
    assert!(page.has_more);
    assert_eq!(page.next_page, Some(2));
    assert_eq!(page.anchor, ANCHOR);
}

#[tokio::test]
async fn ci_lookup_failures_remain_visible_when_no_commits_match() {
    // Given readable history and forbidden CI endpoints for every commit.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    for sha in [RELEASED, PLAIN] {
        for endpoint in ["check-runs", "status"] {
            Mock::given(path(format!(
                "/repos/acme/project/commits/{sha}/{endpoint}"
            )))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        }
    }

    // When selecting only commits with passing CI.
    let page = hw
        .list_commits_filtered(
            "selected-app",
            1,
            20,
            None,
            &CommitFilter {
                ci: Some(CommitCiState::Success),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Then unknown checks cannot pass the filter and each failed lookup is reported.
    assert!(page.commits.is_empty());
    assert_eq!(page.notes.len(), 4);
    for sha in [RELEASED, PLAIN] {
        assert_eq!(
            page.notes
                .iter()
                .filter(|note| note.starts_with(sha))
                .count(),
            2
        );
    }
}
