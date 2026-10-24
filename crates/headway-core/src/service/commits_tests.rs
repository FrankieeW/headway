use std::sync::Arc;

use serde_json::{Value, json};
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::{Track, TrackedApp};
use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::testing::{cli_app, with_github};
use crate::token::MemoryStore;
use crate::{Error, Headway};

const ANCHOR: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const THIRD: &str = "cccccccccccccccccccccccccccccccccccccccc";
const FOURTH: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const ADVANCED: &str = "dddddddddddddddddddddddddddddddddddddddd";

fn headway(server: &MockServer, track: Track) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .secrets(Arc::new(MemoryStore::with_token("fixture-token")))
        .env(ShellEnv::default())
        .api(server.uri())
        .build()
        .unwrap();
    hw.add(with_github(cli_app("selected-app"), |source| {
        crate::GithubSource {
            repo: "acme/project".into(),
            track,
            ..source
        }
    }))
    .unwrap();
    (dir, hw)
}

fn commit(sha: &str, title: &str) -> Value {
    json!({"sha":sha,"html_url":format!("https://github.com/acme/project/commit/{sha}"),
        "commit":{"message":format!("{title}\n\nDetails"),
            "author":{"name":"Builder","date":"2026-10-01T00:00:00Z"},
            "committer":{"date":"2026-10-02T00:00:00Z"}}})
}

async fn page(server: &MockServer, number: u32, commits: Value, link: Option<&str>) {
    let mut response = ResponseTemplate::new(200).set_body_json(commits);
    if let Some(link) = link {
        response = response.insert_header("link", link);
    }
    Mock::given(path("/repos/acme/project/commits"))
        .and(query_param("sha", ANCHOR))
        .and(query_param("page", number.to_string()))
        .and(query_param("per_page", "2"))
        .respond_with(response)
        .expect(1)
        .mount(server)
        .await;
}

fn evidence(name: &str, value: &impl serde::Serialize) {
    if let Some(directory) = std::env::var_os("HEADWAY_HISTORY_EVIDENCE") {
        std::fs::write(
            std::path::Path::new(&directory).join(name),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn branch_advancing_between_pages_keeps_anchor_and_fetches_only_requested_page() {
    // Given the selected repository/branch and two bounded pages at its original tip.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(
        &server,
        Track::Commit {
            branch: Some("feature/history".into()),
        },
    );
    Mock::given(path("/repos/acme/project/commits/feature/history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit(ANCHOR, "First")))
        .expect(1)
        .mount(&server)
        .await;
    page(
        &server,
        1,
        json!([commit(ANCHOR, "First"), commit(SECOND, "Second")]),
        Some("<https://untrusted.invalid/anything?sha=mutable&page=9999>; rel=\"next\""),
    )
    .await;
    let first = hw.list_commits("selected-app", 1, 2, None).await.unwrap();
    // When the branch advances before requesting page two with the returned anchor.
    Mock::given(path("/repos/acme/project/commits/feature/history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit(ADVANCED, "New tip")))
        .with_priority(1)
        .expect(0)
        .mount(&server)
        .await;
    page(
        &server,
        2,
        json!([commit(THIRD, "Third"), commit(FOURTH, "Older fourth")]),
        None,
    )
    .await;
    let second = hw
        .list_commits("selected-app", 2, 2, Some(&first.anchor))
        .await
        .unwrap();
    // Then both pages use the original SHA, retain order and do not infer more from a full last page.
    assert_eq!(first.repo, "acme/project");
    assert_eq!(first.branch, "feature/history");
    assert_eq!(first.anchor, ANCHOR);
    assert_eq!(
        first
            .commits
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        [ANCHOR, SECOND]
    );
    assert_eq!(first.commits[0].title, "First");
    assert_eq!(first.commits[0].body.as_deref(), Some("Details"));
    assert_eq!(first.commits[0].author.as_deref(), Some("Builder"));
    assert_eq!(first.next_page, Some(2));
    assert!(first.has_more);
    assert_eq!(second.anchor, ANCHOR);
    assert_eq!(second.page, 2);
    assert_eq!(second.per_page, 2);
    assert_eq!(
        second
            .commits
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        [THIRD, FOURTH]
    );
    assert_eq!(second.commits[0].sha.as_deref(), Some(THIRD));
    assert_eq!(second.next_page, None);
    assert!(!second.has_more);
    let requests: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| json!({"method":request.method.to_string(),"url":request.url.to_string()}))
        .collect();
    assert_eq!(
        requests.len(),
        3,
        "one tip resolution plus one list request per requested page"
    );
    evidence("page-1.json", &first);
    evidence("page-2.json", &second);
    evidence("requests.json", &requests);
}

#[tokio::test]
async fn release_source_browses_repository_default_branch_without_release_scans() {
    // Given a GitHub release-tracked app and an actual repository default branch.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(
        &server,
        Track::Release {
            prerelease: false,
            tag_filter: None,
        },
    );
    Mock::given(path("/repos/acme/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"full_name":"acme/project","default_branch":"trunk","html_url":"https://github.com/acme/project"})))
        .expect(1).mount(&server).await;
    Mock::given(path("/repos/acme/project/commits/trunk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit(ANCHOR, "Tip")))
        .expect(1)
        .mount(&server)
        .await;
    page(&server, 1, json!([]), None).await;
    // When browsing its first commit page.
    let found = hw.list_commits("selected-app", 1, 2, None).await.unwrap();
    // Then the repository default is resolved and an empty page terminates pagination.
    assert_eq!(found.branch, "trunk");
    assert_eq!(found.anchor, ANCHOR);
    assert!(found.commits.is_empty());
    assert!(!found.has_more);
    assert!(found.next_page.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn invalid_pagination_anchor_and_unsupported_source_fail_before_network() {
    // Given one GitHub and one non-GitHub app with no remote fixtures.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(
        &server,
        Track::Commit {
            branch: Some("main".into()),
        },
    );
    hw.add(TrackedApp {
        source: crate::Source::Npm(crate::NpmSource {
            package: "tool".into(),
            dist_tag: "latest".into(),
        }),
        ..cli_app("node-app")
    })
    .unwrap();
    // When invalid requests are submitted.
    for (number, size, anchor) in [
        (0, 2, None),
        (1, 0, None),
        (1, 101, None),
        (2, 2, None),
        (1, 2, Some("aaaaaaa")),
        (1, 2, Some("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz")),
    ] {
        assert!(matches!(
            hw.list_commits("selected-app", number, size, anchor).await,
            Err(Error::InvalidInput(_))
        ));
    }
    let unsupported = hw.list_commits("node-app", 1, 2, None).await;
    let missing = hw.list_commits("missing-app", 1, 2, None).await;
    // Then local validation distinguishes unsupported/missing apps without any HTTP traffic.
    assert!(matches!(unsupported, Err(Error::InvalidInput(_))));
    assert!(matches!(missing, Err(Error::AppNotFound(_))));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn explicit_anchor_is_normalized_and_does_not_resolve_branch_tip() {
    // Given a supplied immutable anchor and configured branch with no tip fixture.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(
        &server,
        Track::Commit {
            branch: Some("main".into()),
        },
    );
    page(&server, 1, json!([commit(ANCHOR, "Pinned")]), None).await;
    // When even page one is explicitly pinned.
    let found = hw
        .list_commits("selected-app", 1, 2, Some(&ANCHOR.to_ascii_uppercase()))
        .await
        .unwrap();
    // Then one request suffices and the anchor is canonical lowercase.
    assert_eq!(found.anchor, ANCHOR);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[path = "commits_error_tests.rs"]
mod errors;
