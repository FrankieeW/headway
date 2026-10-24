use std::sync::Arc;

use serde_json::{Value, json};
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::testing::cli_app;
use crate::token::MemoryStore;

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn headway(server: &MockServer) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .secrets(Arc::new(MemoryStore::with_token("fixture-token")))
        .env(ShellEnv::default())
        .api(server.uri())
        .build()
        .unwrap();
    hw.add(cli_app("tool")).unwrap();
    (dir, hw)
}

async fn repository(server: &MockServer, tags: Value, releases: Value) {
    for (endpoint, body) in [("tags", tags), ("releases", releases)] {
        Mock::given(path(format!("/repos/owner/tool/{endpoint}")))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
}

async fn ci(server: &MockServer, sha: &str, runs: Value, status: Value) {
    for (endpoint, body) in [("check-runs", runs), ("status", status)] {
        Mock::given(path(format!("/repos/owner/tool/commits/{sha}/{endpoint}")))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
}

fn runs(status: &str, conclusion: Option<&str>) -> Value {
    json!({"total_count":1,"check_runs":[{"id":1,"name":"macOS build","status":status,"conclusion":conclusion,"html_url":"https://github.com/owner/tool/actions/runs/1"}]})
}

fn empty_status() -> Value {
    json!({"state":"pending","total_count":0,"statuses":[]})
}
fn empty_runs() -> Value {
    json!({"total_count":0,"check_runs":[]})
}

#[tokio::test]
async fn matched_peeled_tag_marks_prerelease_and_successful_ci() {
    // Given an annotated release tag whose listed commit SHA is peeled, plus a draft tag.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([
        {"name":"v1.2.3-rc.1","commit":{"sha":SHA}},
        {"name":"nightly","commit":{"sha":SHA}},
        {"name":"v9.0.0","commit":{"sha":OTHER}}
    ]), json!([
        {"id":1,"tag_name":"v1.2.3-rc.1","prerelease":true,"target_commitish":"main","html_url":"https://github.com/owner/tool/releases/tag/v1.2.3-rc.1"},
        {"id":2,"tag_name":"v9.0.0","draft":true}
    ])).await;
    ci(&server, SHA, runs("completed", Some("success")), json!({"state":"success","total_count":1,"statuses":[{"state":"success","target_url":"https://ci.example/job/1"}]})).await;
    // When the public service API annotates this exact commit.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then tag, release and CI facts remain distinct and the wire contract is explicit.
    assert_eq!(found.sha, SHA);
    assert_eq!(found.version.as_deref(), Some("v1.2.3-rc.1"));
    assert_eq!(found.tags, ["v1.2.3-rc.1", "nightly"]);
    assert_eq!(found.is_release, Some(true));
    assert_eq!(
        found.release_url.as_deref(),
        Some("https://github.com/owner/tool/releases/tag/v1.2.3-rc.1")
    );
    assert_eq!(found.ci_state, CommitCiState::Success);
    assert_eq!(found.checks_count, 2);
    assert!(found.ci_url.is_some());
    assert_eq!(serde_json::to_value(&found).unwrap()["ci_state"], "success");
    if let Some(directory) = std::env::var_os("HEADWAY_COMMIT_EVIDENCE") {
        std::fs::write(
            std::path::Path::new(&directory).join("commit-metadata.json"),
            serde_json::to_vec_pretty(&found).unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn pending_failure_no_checks_and_legacy_status_are_distinct() {
    // Given representative GitHub check and legacy status combinations.
    for (checks, status, expected, count) in [
        (
            runs("in_progress", None),
            empty_status(),
            CommitCiState::Pending,
            1,
        ),
        (
            runs("completed", Some("failure")),
            empty_status(),
            CommitCiState::Failure,
            1,
        ),
        (empty_runs(), empty_status(), CommitCiState::NotRun, 0),
        (
            runs("completed", Some("success")),
            json!({"state":"failure","total_count":1,"statuses":[{"state":"failure","target_url":"https://ci.example/failed"}]}),
            CommitCiState::Failure,
            2,
        ),
        (
            empty_runs(),
            json!({"state":"pending","total_count":1,"statuses":[{"state":"pending"}]}),
            CommitCiState::Pending,
            1,
        ),
    ] {
        let server = MockServer::start().await;
        let (_dir, hw) = headway(&server);
        repository(&server, json!([]), json!([])).await;
        ci(&server, SHA, checks, status).await;
        // When requesting metadata.
        let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
        // Then absence is not failure and legacy failures participate in aggregation.
        assert_eq!(found.ci_state, expected);
        assert_eq!(found.checks_count, count);
        assert_eq!(found.is_release, Some(false));
        assert!(found.version.is_none());
    }
}

#[tokio::test]
async fn scope_or_rate_errors_are_unknown_even_if_another_check_failed() {
    // Given either a scope failure or exhausted quota on the checks API.
    for response in [
        ResponseTemplate::new(403),
        ResponseTemplate::new(403).insert_header("x-ratelimit-remaining", "0"),
    ] {
        let server = MockServer::start().await;
        let (_dir, hw) = headway(&server);
        repository(&server, json!([]), json!([])).await;
        ci(
            &server,
            SHA,
            empty_runs(),
            json!({"state":"failure","total_count":1,"statuses":[{"state":"failure"}]}),
        )
        .await;
        Mock::given(path(format!("/repos/owner/tool/commits/{SHA}/check-runs")))
            .respond_with(response)
            .with_priority(1)
            .mount(&server)
            .await;
        // When querying partial CI evidence.
        let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
        // Then permissions/quota issues are never represented as a failed build.
        assert_eq!(found.ci_state, CommitCiState::Unknown);
        assert!(!found.notes.is_empty());
    }
}

#[tokio::test]
async fn release_branch_target_never_matches_an_unrelated_commit() {
    // Given a release targeting a branch (or even this SHA) but a tag on another commit.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(
        &server,
        json!([
            {"name":"v2.0.0","commit":{"sha":OTHER}},
            {"name":"nightly","commit":{"sha":SHA}}
        ]),
        json!([
            {"id":1,"tag_name":"v2.0.0","target_commitish":SHA},
            {"id":2,"tag_name":"nightly","draft":true,"target_commitish":"main"}
        ]),
    )
    .await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    // When annotating the commit matching only a draft/non-version tag.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then neither target_commitish nor a draft creates a release badge.
    assert_eq!(found.tags, ["nightly"]);
    assert_eq!(found.is_release, Some(false));
    assert!(found.version.is_none());
    assert!(found.release_url.is_none());
}

#[tokio::test]
async fn scan_limit_leaves_unmatched_older_releases_unknown() {
    // Given a full bounded tag page, which may omit an older matching tag.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    let tags: Vec<_> = (0..100)
        .map(|index| json!({"name":format!("v{index}.0.0"),"commit":{"sha":OTHER}}))
        .collect();
    repository(&server, json!(tags), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    // When no match appears within the bounded page.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then missing evidence stays unknown instead of claiming no release.
    assert_eq!(found.is_release, None);
    assert!(found.version.is_none());
    assert!(!found.notes.is_empty());
}

#[path = "cache_tests.rs"]
mod caching;

#[path = "edge_tests.rs"]
mod edges;

#[path = "version_tests.rs"]
mod manifest_versions;
