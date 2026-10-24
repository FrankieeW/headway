use super::*;

async fn manifest(server: &MockServer, file: &str, sha: &str, body: &str) {
    Mock::given(path(format!("/repos/owner/tool/contents/{file}")))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn untagged_commits_get_their_own_manifest_version_never_default_branch() {
    // Given two historical versions and a conflicting default-branch response.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    ci(&server, OTHER, empty_runs(), empty_status()).await;
    Mock::given(path("/repos/owner/tool/contents/Cargo.toml"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[package]\nversion='9.9.9'"))
        .with_priority(10)
        .mount(&server)
        .await;
    manifest(
        &server,
        "Cargo.toml",
        SHA,
        "[workspace.package]\nversion='0.1'\n[package]\nversion.workspace=true",
    )
    .await;
    manifest(&server, "Cargo.toml", OTHER, "[package]\nversion='0.2.1'").await;
    // When annotating both historical SHAs.
    let first = hw.commit_metadata("tool", SHA, false).await.unwrap();
    let second = hw.commit_metadata("tool", OTHER, false).await.unwrap();
    // Then their actual manifest versions differ without inventing tags/releases.
    assert_eq!(first.version.as_deref(), Some("0.1"));
    assert_eq!(second.version.as_deref(), Some("0.2.1"));
    for found in [&first, &second] {
        assert_eq!(found.version_source, Some(CommitVersionSource::Manifest));
        assert_eq!(found.is_release, Some(false));
        assert!(found.tags.is_empty());
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().contains("/contents/"))
            .count(),
        2
    );
    if let Some(directory) = std::env::var_os("HEADWAY_COMMIT_EVIDENCE") {
        std::fs::write(
            std::path::Path::new(&directory).join("manifest-versions.json"),
            serde_json::to_vec_pretty(&[first, second]).unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn package_json_fallback_is_cached_immutably_across_refresh_and_sha_aliases() {
    // Given no Cargo manifest and a package version at the exact SHA.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    manifest(
        &server,
        "package.json",
        SHA,
        r#"{"version":"2.3.4-beta.1"}"#,
    )
    .await;
    Mock::given(path("/repos/owner/tool/commits/aaaaaaa"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha":SHA,"commit":{"message":"subject"}})),
        )
        .mount(&server)
        .await;
    // When metadata is loaded through an abbreviation, then refreshed by full SHA.
    let first = hw.commit_metadata("tool", "aaaaaaa", false).await.unwrap();
    let refreshed = hw.commit_metadata("tool", SHA, true).await.unwrap();
    // Then CI/repo refresh does not refetch immutable commit files.
    assert_eq!(first.version.as_deref(), Some("2.3.4-beta.1"));
    assert_eq!(refreshed.version, first.version);
    assert_eq!(
        refreshed.version_source,
        Some(CommitVersionSource::Manifest)
    );
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path().contains("/contents/"))
            .count(),
        2
    );
}

#[tokio::test]
async fn matching_version_tag_takes_priority_without_reading_manifests() {
    // Given a matching version tag; no manifest mocks are needed.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(
        &server,
        json!([{"name":"v1.2.3","commit":{"sha":SHA}}]),
        json!([]),
    )
    .await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    // When a tag already provides the version.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then no fallback reads occur and a tag alone does not become a GitHub release.
    assert_eq!(found.version.as_deref(), Some("v1.2.3"));
    assert_eq!(found.version_source, Some(CommitVersionSource::Tag));
    assert_eq!(found.is_release, Some(false));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().contains("/contents/"))
    );
}

#[tokio::test]
async fn oversized_and_missing_manifests_stay_unknown_with_two_read_limit() {
    // Given an oversized Cargo file and no Node manifest.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, json!([]), json!([])).await;
    ci(&server, SHA, empty_runs(), empty_status()).await;
    manifest(&server, "Cargo.toml", SHA, &"x".repeat(65537)).await;
    // When looking for a version without usable bounded evidence.
    let found = hw.commit_metadata("tool", SHA, false).await.unwrap();
    // Then version/source remain null and there is no third file request.
    assert_eq!(found.version, None);
    assert_eq!(found.version_source, None);
    assert!(!found.notes.is_empty());
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path().contains("/contents/"))
            .count(),
        2
    );
}
