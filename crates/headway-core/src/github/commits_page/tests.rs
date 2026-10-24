use serde_json::json;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn next_relation_is_separate_from_target_url_and_other_relations() {
    // Given GitHub-style Link variants, including malicious query attributes.
    for (header, expected) in [
        (
            "<https://example.invalid/?page=2>; rel=\"next\", <https://example.invalid/?page=9>; rel=\"last\"",
            true,
        ),
        (
            "<https://example.invalid/>; type=\"application/json\"; rel=\"prev next\"",
            true,
        ),
        ("<https://example.invalid/>; rel=next", true),
        ("<https://example.invalid/?rel=next>; rel=\"last\"", false),
        (
            "<https://example.invalid/?x=1;rel=next>; rel=\"prev\"",
            false,
        ),
        ("<https://example.invalid/>; rel=\"next-page\"", false),
        ("not-a-link; rel=next", false),
        ("", false),
    ] {
        // When reading relation metadata.
        let found = has_next_relation(header);
        // Then only the exact next relation signals another local page request.
        assert_eq!(found, expected, "{header}");
    }
}

#[tokio::test]
async fn one_page_preserves_rate_limit_errors() {
    // Given GitHub quota exhaustion on a single commits request.
    let server = MockServer::start().await;
    Mock::given(path("/repos/o/r/commits"))
        .and(query_param("page", "2"))
        .and(query_param("sha", SHA))
        .respond_with(ResponseTemplate::new(403).insert_header("x-ratelimit-remaining", "0"))
        .expect(1)
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    // When requesting the page.
    let result = gh.commits_page(&repo, SHA, 2, 100).await;
    // Then the error remains RateLimited and no pagination retry occurs.
    assert!(matches!(result, Err(Error::RateLimited { .. })));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn response_commit_count_and_bytes_are_bounded() {
    // Given responses violating either the requested count or the body-byte cap.
    for response in [
        ResponseTemplate::new(200).set_body_json(json!([
            {"sha":SHA,"commit":{"message":"one"}}, {"sha":SHA,"commit":{"message":"two"}}
        ])),
        ResponseTemplate::new(200).set_body_string("x".repeat(4 * 1024 * 1024 + 1)),
    ] {
        let server = MockServer::start().await;
        Mock::given(path("/repos/o/r/commits"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let gh = GitHub::new(&server.uri(), None).unwrap();
        let repo = RepoId::parse("o/r").unwrap();
        // When at most one commit was requested.
        let result = gh.commits_page(&repo, SHA, 1, 1).await;
        // Then invalid pages are rejected rather than becoming an oversized history result.
        assert!(matches!(result, Err(Error::Parse { .. })));
    }
}
