use super::*;

#[tokio::test]
async fn repository_scan_and_duplicate_rows_are_shared_then_refresh_bypasses_both() {
    // Given two rows in the same repository and simultaneous duplicate requests.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    ci(&server, OTHER, empty_runs(), empty_status()).await;
    // When loading duplicate and different rows, followed by explicit refresh.
    let (a, duplicate, b) = tokio::join!(
        hw.commit_metadata("tool", SHA, false),
        hw.commit_metadata("tool", SHA, false),
        hw.commit_metadata("tool", OTHER, false)
    );
    assert_eq!(a.unwrap().checked_at, duplicate.unwrap().checked_at);
    assert!(b.is_ok());
    let before = server.received_requests().await.unwrap();
    assert_eq!(
        before.len(),
        10,
        "two repository scans plus two CI endpoints and two absent manifests per unique row"
    );
    hw.commit_metadata("tool", SHA, true).await.unwrap();
    // Then refresh adds a repository scan plus CI queries rather than returning cached data.
    let after = server.received_requests().await.unwrap();
    assert_eq!(after.len(), 14);
    assert_eq!(
        after
            .iter()
            .filter(|request| request.url.path().ends_with("/tags"))
            .count(),
        2
    );
}

#[tokio::test]
async fn abbreviated_sha_is_resolved_and_invalid_input_is_rejected_locally() {
    // Given a GitHub resolver for one abbreviated commit.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    Mock::given(path("/repos/owner/tool/commits/aaaaaaa"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha":SHA,"commit":{"message":"subject"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    // When resolving the abbreviation and rejecting non-SHA inputs.
    let found = hw.commit_metadata("tool", "AAAAAAA", false).await.unwrap();
    for invalid in [
        "main",
        "abc123",
        "zzzzzzz",
        "aaaaaaa;rm",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        assert!(matches!(
            hw.commit_metadata("tool", invalid, false).await,
            Err(Error::InvalidInput(_))
        ));
    }
    // Then only the valid abbreviation causes any HTTP activity.
    assert_eq!(found.sha, SHA);
    assert_eq!(server.received_requests().await.unwrap().len(), 7);
}

#[tokio::test]
async fn non_github_source_is_rejected_without_network_or_token_lookup() {
    // Given a tracked npm source and no metadata fixtures.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    let app = crate::TrackedApp {
        source: crate::Source::Npm(crate::NpmSource {
            package: "tool".into(),
            dist_tag: "latest".into(),
        }),
        ..cli_app("npm-tool")
    };
    hw.add(app).unwrap();
    // When requesting commit metadata for that source.
    let result = hw.commit_metadata("npm-tool", SHA, false).await;
    // Then the public API reports unsupported input locally.
    assert!(matches!(result, Err(Error::InvalidInput(_))));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn ci_expires_after_thirty_seconds_and_repository_after_five_minutes() {
    // Given populated caches with manually advanced monotonic ages (no sleeps).
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    hw.commit_metadata("tool", SHA, false).await.unwrap();
    let repository = hw.commit_metadata_cache.repository("owner/tool").await;
    let row = repository.row(SHA).await;
    row.lock().await.as_mut().unwrap().at = std::time::Instant::now()
        .checked_sub(cache::COMMIT_TTL)
        .expect("fixture clock can represent the expired cache timestamp");
    // When the CI cache expires, then the repository cache expires.
    hw.commit_metadata("tool", SHA, false).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 8);
    repository.snapshot.lock().await.as_mut().unwrap().at = std::time::Instant::now()
        .checked_sub(cache::REPOSITORY_TTL)
        .expect("fixture clock can represent the expired cache timestamp");
    hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then even a fresh row cannot extend tag/release cache beyond five minutes.
    assert_eq!(server.received_requests().await.unwrap().len(), 12);
}
