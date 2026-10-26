use super::to_brew::github_to_brew;
use crate::{CancellationToken, repo::RepoId, testing::FakeTools};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

#[tokio::test]
async fn installed_and_all_searches_start_before_any_probe_finishes() {
    // Given a barrier that releases no probe until the installed lookup and all four searches arrive.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tools = FakeTools::new().tool(
        "brew",
        r#"
exec /usr/bin/curl --noproxy '*' -fsS "$MATCH_TEST_SERVER/$1/$2/$3"
"#,
    );
    let env = tools.env().with_var(
        "MATCH_TEST_SERVER",
        format!("http://{}", listener.local_addr().unwrap()),
    );
    let barrier = async {
        let mut requests = Vec::new();
        for _ in 0..5 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            stream.read_line(&mut request).await.unwrap();
            requests.push((request, stream));
        }
        for (request, mut stream) in requests {
            let body = if request.contains("/info/--installed/") {
                r#"{"formulae":[{"name":"renamed","homepage":"https://github.com/acme/tool-app"}]}"#
            } else {
                ""
            };
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            stream.shutdown().await.unwrap();
        }
    };
    let cancel = CancellationToken::new();
    let repo = RepoId::parse("acme/tool-app").unwrap();
    // When matching runs, all probes must reach the barrier together (timeout only guards deadlock).
    let (report, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(github_to_brew(&cancel, &env, None, &repo, true), barrier)
    })
    .await
    .expect("all five probes must run concurrently");
    // Then the installed match is preserved.
    let report = report.unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.packages[0].package, "renamed");
}
