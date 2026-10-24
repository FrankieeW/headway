use super::*;

#[tokio::test]
async fn full_release_page_is_unknown_but_a_proven_match_remains_true() {
    // Given 100 releases, including one provably matching a tag on this commit.
    for matched in [false, true] {
        let server = MockServer::start().await;
        let (_dir, hw) = headway(&server);
        let releases: Vec<_> = (0..100)
            .map(|index| json!({"id":index,"tag_name":format!("v{index}.0.0")}))
            .collect();
        let tags = if matched {
            json!([{"name":"v1.0.0","commit":{"sha":SHA}}])
        } else {
            json!([])
        };
        repository(&server, tags, json!(releases)).await;
        ci(&server, SHA, empty_runs(), empty_status()).await;
        // When the bounded release page may omit older releases.
        let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
        // Then only positive tag/commit evidence permits a definitive badge.
        assert_eq!(found.is_release, matched.then_some(true));
    }
}

#[tokio::test]
async fn partial_check_runs_do_not_claim_success() {
    // Given a check page declaring more results than it contains.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    let mut checks = runs("completed", Some("success"));
    checks["total_count"] = json!(101);
    ci(&server, SHA, checks, empty_status()).await;
    // When aggregation lacks the remaining runs.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then the first successful run cannot establish overall success.
    assert_eq!(found.ci_state, CommitCiState::Unknown);
    assert_eq!(found.checks_count, 101);
    assert!(!found.notes.is_empty());
}

#[tokio::test]
async fn legacy_endpoint_failure_and_tag_errors_remain_unknown() {
    // Given valid check success but failed legacy/tag/release requests.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    ci(
        &server,
        SHA,
        runs("completed", Some("success")),
        empty_status(),
    )
    .await;
    Mock::given(path(format!("/repos/owner/tool/commits/{SHA}/status")))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&server)
        .await;
    // When unavailable metadata is requested through the public API.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then missing optional evidence is neither a failed check nor a negative release claim.
    assert_eq!(found.ci_state, CommitCiState::Unknown);
    assert_eq!(found.is_release, None);
    assert!(found.version.is_none());
    assert_eq!(found.notes.len(), 3);
}

#[tokio::test]
async fn refreshing_another_row_cannot_extend_original_repository_snapshot_age() {
    // Given a fresh CI row whose original tag/release snapshot has expired.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    ci(&server, OTHER, empty_runs(), empty_status()).await;
    hw.commit_metadata("tool", SHA, false).await.unwrap();
    let repo_cache = hw.commit_metadata_cache.repository("owner/tool").await;
    let row = repo_cache.row(SHA).await;
    row.lock().await.as_mut().unwrap().repository_at = std::time::Instant::now()
        .checked_sub(cache::REPOSITORY_TTL)
        .expect("fixture clock can represent the expired cache timestamp");
    hw.commit_metadata("tool", OTHER, true).await.unwrap();
    let before = server.received_requests().await.unwrap().len();
    // When revisiting the original row after another row refreshed the repository cache.
    hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then the old row must be recomputed, despite the now-fresh shared snapshot.
    assert_eq!(server.received_requests().await.unwrap().len(), before + 2);
}
