use super::*;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::oneshot;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn unfinished_response(
    response: &str,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response = response.to_owned();
    let (release, wait) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
        }
        stream.write_all(response.as_bytes()).await.unwrap();
        wait.await.unwrap();
    });
    (format!("http://{address}"), release, task)
}

#[tokio::test]
async fn declared_oversize_returns_without_waiting_for_body() {
    // Given an oversized Content-Length, with no body sent until the client returns.
    let (url, release, server) =
        unfinished_response("HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\n").await;
    let gh = GitHub::new(&url, None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    // When requesting six bytes maximum.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        gh.file_text_bounded(&repo, "README.md", 6),
    )
    .await;
    release.send(()).unwrap();
    server.await.unwrap();
    // Then it rejects from headers rather than waiting for the absent body.
    assert_eq!(result.unwrap().unwrap(), BoundedFileText::TooLarge);
}

#[tokio::test]
async fn chunked_oversize_returns_before_stream_finishes() {
    // Given a chunked response with no Content-Length and no terminating chunk.
    let (url, release, server) = unfinished_response(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nabcd\r\n4\r\nefgh\r\n",
    )
    .await;
    let gh = GitHub::new(&url, None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    // When the second chunk crosses the six-byte cap.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        gh.file_text_bounded(&repo, "README.md", 6),
    )
    .await;
    release.send(()).unwrap();
    server.await.unwrap();
    // Then it returns before EOF; a whole-body reader would time out.
    assert_eq!(result.unwrap().unwrap(), BoundedFileText::TooLarge);
}

#[tokio::test]
async fn bounded_text_preserves_exact_limit_missing_and_http_errors() {
    // Given successful, missing, auth, rate-limit and upstream-failure responses.
    let server = MockServer::start().await;
    for (name, response) in [
        (
            "exact",
            ResponseTemplate::new(200).set_body_string("abcdef"),
        ),
        ("missing", ResponseTemplate::new(404)),
        (
            "auth",
            ResponseTemplate::new(401).set_body_string("x".repeat(1000)),
        ),
        (
            "rate",
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .set_body_string("x".repeat(1000)),
        ),
        (
            "upstream",
            ResponseTemplate::new(500).set_body_string("x".repeat(1000)),
        ),
    ] {
        Mock::given(path(format!("/repos/o/r/contents/{name}")))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    // When reading each response with the same bounded API.
    let mut results = Vec::new();
    for name in ["exact", "missing", "auth", "rate", "upstream"] {
        results.push(gh.file_text_bounded(&repo, name, 6).await);
    }
    // Then size limits do not hide established status semantics.
    assert_eq!(
        results[0].as_ref().unwrap(),
        &BoundedFileText::Text("abcdef".into())
    );
    assert_eq!(results[1].as_ref().unwrap(), &BoundedFileText::Missing);
    assert!(matches!(&results[2], Err(Error::Auth(_))));
    assert!(matches!(&results[3], Err(Error::RateLimited { .. })));
    assert!(matches!(
        &results[4],
        Err(Error::GitHub { status: 500, .. })
    ));
}

#[tokio::test]
async fn truncated_body_remains_a_network_error() {
    // Given EOF before the advertised small body is complete.
    let (url, release, server) =
        unfinished_response("HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nabc").await;
    release.send(()).unwrap();
    let gh = GitHub::new(&url, None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    // When reading within the cap.
    let result = gh.file_text_bounded(&repo, "README.md", 6).await;
    server.await.unwrap();
    // Then a broken stream is not reported as missing or oversized.
    assert!(matches!(result, Err(Error::Network(_))));
}

#[tokio::test]
async fn invalid_utf8_success_is_parse_error_but_auth_status_is_preserved() {
    // Given invalid text in both a success response and an auth error body.
    let server = MockServer::start().await;
    for (name, status) in [("invalid", 200), ("invalid-auth", 401)] {
        Mock::given(path(format!("/repos/o/r/contents/{name}")))
            .respond_with(ResponseTemplate::new(status).set_body_bytes(vec![255]))
            .mount(&server)
            .await;
    }
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    // When reading the bounded responses.
    let invalid = gh.file_text_bounded(&repo, "invalid", 6).await;
    let auth = gh.file_text_bounded(&repo, "invalid-auth", 6).await;
    // Then malformed source text is explicit, while authentication retains priority.
    assert!(matches!(invalid, Err(Error::Parse { .. })));
    assert!(matches!(auth, Err(Error::Auth(_))));
}
