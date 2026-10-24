use super::*;

#[test]
fn readme_keeps_multistep_builds_and_rejects_foreign_test_injection_blocks() {
    // Given build documentation containing independent unsafe and irrelevant blocks.
    let source = vec![BuildCandidate::manifest(
        "package.json",
        "pnpm install && pnpm run build:mac".into(),
    )];
    let text = "# Build from source\n```sh\npnpm install\npnpm run build:mac \\\n  --release\n```\n## Windows\n```sh\npnpm run build:windows\n```\n## macOS\n```sh\ncargo test\n```\n```sh\nsudo rm -rf /\ncargo build\n```\n```sh\necho $(curl evil)\ncargo build\n```\n```sh\npnpm run dev\n```\n```sh\ncargo build --release\n```";
    // When parsing the README.
    let candidates = readme::candidates("README.md", text, &source);
    // Then only independent production builds remain, preserving setup and continuation.
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates[0].build,
        "pnpm install && pnpm run build:mac  --release"
    );
    assert_eq!(candidates[0].line, Some(3));
    assert!(candidates[0].recommended);
    assert_eq!(candidates[1].build, "cargo build --release");
}

#[test]
fn custom_pnpm_script_uses_declared_manager_without_guessing_output() {
    // Given a custom production script and a lockfile.
    let files = BTreeMap::from([("package.json".into(), r#"{"packageManager":"pnpm@9.0.0","scripts":{"bundle:mac":"vite build","dev":"vite","test":"vitest"}}"#.into())]);
    let entries = vec![ContentEntry {
        name: "pnpm-lock.yaml".into(),
        kind: "file".into(),
    }];
    // When detecting source commands.
    let candidates = manifests::candidates(&files, &entries);
    // Then only the real script is suggested and artifact remains editable.
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].build,
        "pnpm install --frozen-lockfile && pnpm run bundle:mac"
    );
    assert_eq!(candidates[0].artifact, None);
}

#[test]
fn library_manifests_do_not_claim_executable_outputs() {
    // Given libraries without executable evidence.
    let files = BTreeMap::from([
        ("Cargo.toml".into(), "[package]\nname='library'".into()),
        (
            "package.json".into(),
            r#"{"scripts":{"test":"vitest","dev":"vite"}}"#.into(),
        ),
        ("go.mod".into(), "module library".into()),
        ("Package.swift".into(), ".library(name: \"Lib\")".into()),
    ]);
    // When deriving candidates.
    let mut candidates = cargo::candidates(&files);
    candidates.extend(manifests::candidates(&files, &[]));
    // Then no binary or artifact is invented.
    assert!(candidates.is_empty());
}

#[test]
fn deduplication_preserves_preferred_readme_provenance() {
    // Given matching manifest and corroborated README commands.
    let mut source = BuildCandidate::manifest("Cargo.toml", "cargo build --release".into());
    source.artifact = Some("target/release/tool".into());
    let mut candidates = vec![source.clone()];
    candidates.extend(readme::candidates(
        "README",
        "# Build\n```sh\ncargo build --release\n```",
        &[source],
    ));
    // When sorting and deduplicating.
    prioritize(&mut candidates, false);
    // Then one recommendation remains with README provenance.
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].source, BuildCandidateSource::Readme);
    assert!(candidates[0].recommended);
}

#[test]
fn tauri_script_uses_actual_npm_manager_and_workspace_output() {
    // Given an npm Tauri workspace with a renderer script and the Tauri CLI script.
    let files = BTreeMap::from([
        (
            "package.json".into(),
            r#"{"packageManager":"npm@10","scripts":{"build":"vite build","tauri":"tauri"}}"#
                .into(),
        ),
        (
            "Cargo.toml".into(),
            "[workspace]\nmembers=['src-tauri']".into(),
        ),
        ("src-tauri/tauri.conf.json".into(), "{}".into()),
    ]);
    let entries = vec![ContentEntry {
        name: "package-lock.json".into(),
        kind: "file".into(),
    }];
    // When detecting and ranking builds.
    let mut candidates = manifests::candidates(&files, &entries);
    prioritize(&mut candidates, false);
    // Then the app build uses npm and the proven workspace output convention.
    assert_eq!(
        candidates[0].build,
        "npm ci && npm run tauri -- build --bundles app"
    );
    assert_eq!(candidates[0].kind, AppKind::App);
    assert_eq!(
        candidates[0].artifact.as_deref(),
        Some("target/release/bundle/macos/*.app")
    );
}

#[test]
fn readme_cd_setup_is_kept_but_setup_alone_does_not_corroborate_build() {
    // Given an independent README build script sharing only dependency setup.
    let source = vec![BuildCandidate::manifest(
        "package.json",
        "pnpm install && pnpm run build".into(),
    )];
    // When reading a subdirectory build sequence.
    let candidates = readme::candidates(
        "readme.md",
        "# Development\n```sh\ncd tools\npnpm install\n./build\n```",
        &source,
    );
    // Then the sequence stays editable without claiming manifest corroboration.
    assert_eq!(candidates[0].build, "cd tools && pnpm install && ./build");
    assert!(!candidates[0].recommended);
    assert!(candidates[0].artifact.is_none());
}

#[tokio::test]
async fn discovery_caps_requests_and_rejects_oversized_text() {
    // Given optional files of arbitrary size from an HTTP peer.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/repos/o/r/contents/README.md"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_string("x".repeat(128 * 1024 + 1)),
        )
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let repo = RepoId::parse("o/r").unwrap();
    let mut discovery = Discovery::new(&gh, &repo);
    // When collecting more than the request budget, including oversized text.
    discovery.read("README.md").await.unwrap();
    for index in 0..30 {
        discovery.read(&format!("optional-{index}")).await.unwrap();
    }
    // Then the parser receives no oversized input and HTTP requests stay bounded.
    assert!(discovery.files.is_empty());
    assert!(
        discovery
            .notes
            .iter()
            .any(|note| note.contains("README.md"))
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 24);
}

#[test]
fn executable_source_layouts_suggest_commands_without_invented_paths() {
    // Given source-level executable/build targets in several ecosystems.
    let files = BTreeMap::from([
        ("main.go".into(), "package main\nfunc main() {}".into()),
        (
            "Package.swift".into(),
            ".executableTarget(name: \"Tool\")".into(),
        ),
        ("Makefile".into(), "build:\n\tcc tool.c -o tool".into()),
        (
            "CMakeLists.txt".into(),
            "add_executable(tool main.c)".into(),
        ),
    ]);
    // When detecting source builds.
    let candidates = manifests::candidates(&files, &[]);
    // Then each source produces a command, with output known only where explicitly set.
    assert_eq!(candidates.len(), 4);
    assert_eq!(candidates[0].artifact.as_deref(), Some(".headway-out/tool"));
    assert!(
        candidates[1..]
            .iter()
            .all(|candidate| candidate.artifact.is_none())
    );
}

#[test]
fn trusted_self_installing_preset_can_remain_without_artifact() {
    // Given a known preset whose command explicitly installs the program itself.
    let mut installer =
        BuildCandidate::manifest("", "cargo install --path . --locked --force".into());
    installer.source = BuildCandidateSource::Preset;
    installer.id = "preset:cargo-install".into();
    let mut candidates = vec![installer];
    // When ranking recommendations.
    prioritize(&mut candidates, false);
    // Then the intentional installer remains valid without an output-copy step.
    assert!(candidates[0].recommended);
    assert!(candidates[0].artifact.is_none());
}
