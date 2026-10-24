use super::*;

#[tokio::test]
async fn readme_with_unknown_output_cannot_override_proven_manifest_artifact() {
    // Given an explicit Cargo binary and a README altering its output location.
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name":"Cargo.toml","type":"file"},{"name":"README.md","type":"file"}]),
        json!([]),
    )
    .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "# Build\n```sh\ncargo build --release --locked --target-dir custom-output\n```",
        ))
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When inspection ranks the recipes.
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    // Then only the manifest's known output is selected, while README remains editable.
    assert_eq!(
        found.build_candidates[0].source,
        BuildCandidateSource::Manifest
    );
    assert!(found.build_candidates[0].recommended);
    let readme = found
        .build_candidates
        .iter()
        .find(|candidate| candidate.source == BuildCandidateSource::Readme)
        .unwrap();
    assert!(!readme.recommended);
    assert!(readme.artifact.is_none());
    assert!(
        matches!(&found.suggestion.github().unwrap().install, Install::Source(source) if source.artifact.as_deref() == Some("target/release/rg") && source.build == "cargo build --release --locked")
    );
    found.suggestion.validate().unwrap();
}

#[tokio::test]
async fn unknown_readme_output_keeps_the_installable_release_fallback() {
    // Given a documented build-only command and a usable release asset.
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name":"README.md","type":"file"}]),
        releases(),
    )
    .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_string("# Build\n```sh\n./build.sh\n```"))
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When inspecting without known build output evidence.
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    // Then the build-only suggestion cannot replace an installable release.
    assert_eq!(found.build_candidates.len(), 1);
    assert!(!found.build_candidates[0].recommended);
    assert!(matches!(
        found.suggestion.github().unwrap().install,
        Install::Asset(_)
    ));
    found.suggestion.validate().unwrap();
}
