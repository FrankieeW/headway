use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::redact::scrub_urls;
use super::*;
use crate::repo::RepoId;

fn repo() -> RepoId {
    RepoId::parse("octo/tool").unwrap()
}

fn client(server: &MockServer, token: Option<&str>) -> GitHub {
    GitHub::new(&server.uri(), token.map(str::to_owned)).unwrap()
}

#[tokio::test]
async fn sends_token_and_api_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Bearer ghp_test"))
        .and(header("x-github-api-version", "2022-11-28"))
        .and(header("accept", "application/vnd.github+json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"login": "frankie", "name": null})),
        )
        .mount(&server)
        .await;
    let user = client(&server, Some("ghp_test")).user().await.unwrap();
    assert_eq!(user.login, "frankie");
    assert_eq!(user.name, None);
}

#[test]
fn api_base_policy_accepts_https_and_loopback_http_only() {
    for api in [
        "https://api.github.com",
        "https://ghe.example.com/api/v3/",
        "http://127.0.0.1:9",
        "http://localhost:8080",
        "http://[::1]:9000",
    ] {
        assert!(GitHub::new(api, None).is_ok(), "{api}");
    }
    for api in [
        "http://api.github.com",
        "http://ghe.example.com/api/v3",
        "ftp://example.com",
        "ssh://github.com",
        "https://user:pass@github.com",
        "https://api.github.com?token=x",
        "https://api.github.com#fragment",
        "not a url",
        "",
    ] {
        assert!(GitHub::new(api, None).is_err(), "{api}");
    }
    // Loopback mock servers keep working; trailing slashes are normalized away.
    assert_eq!(
        GitHub::new("http://127.0.0.1:1234/", None).unwrap().api,
        "http://127.0.0.1:1234"
    );
    assert_eq!(
        GitHub::new("https://ghe.example.com/api/v3/", None)
            .unwrap()
            .api,
        "https://ghe.example.com/api/v3"
    );
}

#[tokio::test]
async fn maps_error_statuses() {
    let server = MockServer::start().await;
    Mock::given(path("/user"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(path("/rate_limit"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1700000000")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/tags"))
        .respond_with(
            ResponseTemplate::new(500)
                .insert_header("retry-after", "0")
                .set_body_string("upstream exploded"),
        )
        .mount(&server)
        .await;

    let gh = client(&server, None);
    assert!(matches!(gh.user().await, Err(Error::Auth(msg)) if msg.contains("Bad credentials")));
    assert!(matches!(
        gh.rate_limit().await,
        Err(Error::RateLimited { reset }) if reset.starts_with("2023-11-14")
    ));
    let missing = gh.repo(&repo()).await.unwrap_err();
    assert!(matches!(&missing, Error::RemoteNotFound(msg) if msg.contains("no GitHub token")));
    assert!(matches!(
        gh.tags(&repo()).await,
        Err(Error::GitHub { status: 500, message }) if message == "upstream exploded"
    ));
}

#[tokio::test]
async fn parses_commits_and_comparisons() {
    let server = MockServer::start().await;
    let commit = |sha: &str, message: &str| {
        json!({
            "sha": sha,
            "html_url": format!("https://github.com/octo/tool/commit/{sha}"),
            "commit": {
                "message": message,
                "author": {"name": "Ada", "date": "2026-10-01T10:00:00Z"},
                "committer": {"name": "GitHub", "date": "2026-10-01T10:05:00Z"}
            },
            "author": null
        })
    };
    Mock::given(path("/repos/octo/tool/compare/aaa...main"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ahead", "ahead_by": 2, "behind_by": 0,
            "commits": [commit("bbb", "first\n\nbody"), commit("ccc", "second")]
        })))
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/commits"))
        .and(query_param("sha", "feature/x+y"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([commit("ddd", "x")])))
        .mount(&server)
        .await;

    let gh = client(&server, None);
    let comparison = gh.compare(&repo(), "aaa", "main").await.unwrap();
    assert_eq!(comparison.status, "ahead");
    assert_eq!(comparison.ahead_by, 2);
    assert_eq!(comparison.commits[0].commit.message, "first\n\nbody");
    assert!(comparison.commits[0].author.is_none());

    let commits = gh.commits(&repo(), "feature/x+y", 5).await.unwrap();
    assert_eq!(commits[0].sha, "ddd");
}

#[tokio::test]
async fn downloads_assets_with_digest_and_progress() {
    let server = MockServer::start().await;
    Mock::given(path("/repos/octo/tool/releases/assets/7"))
        .and(header("accept", "application/octet-stream"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tool.tar.gz");
    let asset = Asset {
        id: 7,
        name: "tool.tar.gz".into(),
        size: 5,
        browser_download_url: None,
        digest: None,
    };
    let mut progress = Vec::new();
    let digest = client(&server, None)
        .download_asset(&repo(), &asset, &dest, &mut |done, total| {
            progress.push((done, total));
        })
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
    assert_eq!(
        digest,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(progress.last(), Some(&(5, Some(5))));
}

#[tokio::test]
async fn downloads_are_capped_by_size() {
    let server = MockServer::start().await;
    Mock::given(path("/repos/octo/tool/releases/assets/7"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tool.tar.gz");
    let asset = Asset {
        id: 7,
        name: "tool.tar.gz".into(),
        size: 5,
        browser_download_url: None,
        digest: None,
    };
    let error = client(&server, None)
        .download_asset_bounded(&repo(), &asset, &dest, &mut |_, _| {}, 4)
        .await
        .unwrap_err();
    assert!(matches!(&error, Error::NoAsset(message) if message.contains("download limit")));
    assert!(!dest.exists());
}

#[tokio::test]
async fn missing_files_are_none() {
    let server = MockServer::start().await;
    Mock::given(path("/repos/octo/tool/contents/Cargo.toml"))
        .and(header("accept", "application/vnd.github.raw+json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[package]\nname = \"tool\"\n"))
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/contents/go.mod"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    let gh = client(&server, Some("t"));
    assert!(
        gh.file_text(&repo(), "Cargo.toml")
            .await
            .unwrap()
            .unwrap()
            .contains("tool")
    );
    assert_eq!(gh.file_text(&repo(), "go.mod").await.unwrap(), None);
}

#[tokio::test]
async fn rate_limited_requests_retry_after_the_hinted_delay() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(json!({"message": "slow down"})),
        )
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"login": "frankie", "name": null})),
        )
        .mount(&server)
        .await;

    let user = client(&server, None).user().await.unwrap();
    assert_eq!(user.login, "frankie");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn transient_server_errors_are_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after", "0")
                .set_body_string("unavailable"),
        )
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"login": "frankie", "name": null})),
        )
        .mount(&server)
        .await;

    assert_eq!(client(&server, None).user().await.unwrap().login, "frankie");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn server_errors_give_up_after_bounded_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(502)
                .insert_header("retry-after", "0")
                .set_body_string("bad gateway"),
        )
        .mount(&server)
        .await;

    let error = client(&server, None).user().await.unwrap_err();
    assert!(matches!(error, Error::GitHub { status: 502, .. }));
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn exhausted_primary_rate_limit_fails_fast() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "4102444800")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;

    let error = client(&server, None).user().await.unwrap_err();
    assert!(matches!(error, Error::RateLimited { .. }));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn secondary_rate_limit_with_retry_hint_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "4000")
                .insert_header("retry-after", "0")
                .set_body_json(json!({"message": "secondary rate limit"})),
        )
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"login": "frankie", "name": null})),
        )
        .mount(&server)
        .await;

    assert_eq!(client(&server, None).user().await.unwrap().login, "frankie");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn oversized_json_bodies_are_rejected() {
    let server = MockServer::start().await;
    Mock::given(path("/repos/octo/tool"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({"full_name": "octo/tool", "default_branch": "main"}).to_string(),
        ))
        .mount(&server)
        .await;
    let result = client(&server, None)
        .get_json_with_limit::<RepoInfo>(
            "/repos/octo/tool",
            "application/vnd.github+json",
            "repository",
            8,
        )
        .await;
    assert!(matches!(result, Err(Error::Parse { .. })));
}

#[tokio::test]
async fn error_bodies_are_bounded_and_redacted() {
    let server = MockServer::start().await;
    let body = format!(
        "failed at https://api.github.com/repos/octo/tool?access_token=supersecret {}",
        "x".repeat(64 * 1024)
    );
    Mock::given(path("/repos/octo/tool"))
        .respond_with(
            ResponseTemplate::new(500)
                .insert_header("retry-after", "0")
                .set_body_string(body),
        )
        .mount(&server)
        .await;

    let error = client(&server, None).repo(&repo()).await.unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, Error::GitHub { status: 500, .. }));
    assert!(!message.contains("supersecret"), "{message}");
    assert!(message.len() < 1000, "{message}");
}

#[test]
fn scrub_urls_removes_credentials_queries_and_fragments() {
    let scrubbed = scrub_urls(
        "error sending request for url (https://user:pass@api.github.com/repos/o/r?sha=abc#frag)",
    );
    assert!(!scrubbed.contains("pass"), "{scrubbed}");
    assert!(!scrubbed.contains("sha="), "{scrubbed}");
    assert!(scrubbed.contains("<redacted>@api.github.com"), "{scrubbed}");
    // Text without URLs is untouched.
    assert_eq!(scrub_urls("no url here"), "no url here");
    // Plain-token URLs keep their path but lose the query.
    assert_eq!(
        scrub_urls("GET http://127.0.0.1:9/x?token=1 failed"),
        "GET http://127.0.0.1:9/x?<redacted> failed"
    );
}

#[tokio::test]
async fn pagination_requests_local_pages_only() {
    let server = MockServer::start().await;
    let tags = |start: usize| {
        let items: Vec<_> = (start..start + 100)
            .map(|index| json!({"name": format!("v{index}"), "commit": {"sha": format!("{index:040x}")}}))
            .collect();
        json!(items)
    };
    Mock::given(path("/repos/octo/tool/tags"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    "<http://169.254.169.254/latest/meta-data>; rel=\"next\"",
                )
                .set_body_json(tags(0)),
        )
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/tags"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "v100", "commit": {"sha": format!("{:040x}", 100)}}
        ])))
        .mount(&server)
        .await;

    let found = client(&server, None).tags_page(&repo()).await.unwrap();
    assert_eq!(found.items.len(), 101);
    assert!(!found.truncated);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() == "/repos/octo/tool/tags")
    );
    assert!(
        requests[1]
            .url
            .query_pairs()
            .any(|(key, value)| key.as_ref() == "page" && value.as_ref() == "2")
    );
}

#[tokio::test]
async fn pagination_stops_at_the_bounded_page_cap() {
    let server = MockServer::start().await;
    let items: Vec<_> = (0..100)
        .map(|index| json!({"name": format!("v{index}"), "commit": {"sha": format!("{index:040x}")}}))
        .collect();
    Mock::given(path("/repos/octo/tool/tags"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", "</repos/octo/tool/tags?page=99>; rel=\"next\"")
                .set_body_json(json!(items)),
        )
        .mount(&server)
        .await;

    let found = client(&server, None).tags_page(&repo()).await.unwrap();
    assert_eq!(found.items.len(), 500);
    // The server still advertises another page: the scan is explicitly truncated,
    // not silently presented as the complete tag set.
    assert!(found.truncated);
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}

#[tokio::test]
async fn pagination_rejects_oversized_hostile_pages() {
    let server = MockServer::start().await;
    let items: Vec<_> = (0..101)
        .map(|index| json!({"name": format!("v{index}"), "commit": {"sha": format!("{index:040x}")}}))
        .collect();
    Mock::given(path("/repos/octo/tool/tags"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", "</repos/octo/tool/tags?page=2>; rel=\"next\"")
                .set_body_json(json!(items)),
        )
        .mount(&server)
        .await;

    // A page larger than the requested per_page is rejected instead of being
    // accepted and silently trimmed by a global item cap.
    let error = client(&server, None).tags(&repo()).await.unwrap_err();
    assert!(matches!(error, Error::Parse { .. }));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[test]
fn encodes_refs() {
    assert_eq!(encode("release/1.0"), "release/1.0");
    assert_eq!(encode("a b#c"), "a%20b%23c");
    assert_eq!(to_hex(&[0, 15, 255]), "000fff");
}
