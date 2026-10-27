use super::*;

#[test]
fn tauri_2_local_build_is_recommended_when_updater_artifacts_are_enabled() {
    let files = BTreeMap::from([
        ("package.json".into(), r#"{"packageManager":"pnpm@10","scripts":{"build":"pnpm tauri build","tauri":"tauri"}}"#.into()),
        ("src-tauri/tauri.conf.json".into(), r#"{"$schema":"https://schema.tauri.app/config/2","bundle":{"createUpdaterArtifacts":true}}"#.into()),
    ]);
    let entries = vec![ContentEntry {
        name: "pnpm-lock.yaml".into(),
        kind: "file".into(),
    }];
    let mut candidates = manifests::candidates(&files, &entries);
    prioritize(&mut candidates, false);
    let local = &candidates[0];
    assert!(local.recommended);
    assert!(
        local
            .build
            .contains("--config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'")
    );
    assert_eq!(
        local.artifact.as_deref(),
        Some("src-tauri/target/release/bundle/macos/*.app")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.build.ends_with("pnpm run build"))
    );
}

#[test]
fn unknown_tauri_version_keeps_original_commands() {
    let files = BTreeMap::from([
        (
            "package.json".into(),
            r#"{"scripts":{"tauri":"tauri"}}"#.into(),
        ),
        (
            "src-tauri/tauri.conf.json".into(),
            r#"{"tauri":{"bundle":{"active":true}}}"#.into(),
        ),
    ]);
    let candidates = manifests::candidates(&files, &[]);
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
    );
}

#[test]
fn custom_build_script_is_not_bypassed_when_tauri_2_is_detected() {
    let files = BTreeMap::from([
        (
            "package.json".into(),
            r#"{"scripts":{"build":"node prepare.js && tauri build","tauri":"tauri"}}"#.into(),
        ),
        (
            "src-tauri/tauri.conf.json".into(),
            r#"{"$schema":"https://schema.tauri.app/config/2"}"#.into(),
        ),
    ]);
    let candidates = manifests::candidates(&files, &[]);
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.build.ends_with("npm run build"))
    );
}

#[test]
fn tauri_2_npm_build_passes_override_to_tauri() {
    let files = BTreeMap::from([
        (
            "package.json".into(),
            r#"{"packageManager":"npm@10","scripts":{"tauri":"tauri"}}"#.into(),
        ),
        (
            "src-tauri/tauri.conf.json".into(),
            r#"{"$schema":"https://schema.tauri.app/config/2"}"#.into(),
        ),
    ]);
    let candidates = manifests::candidates(&files, &[]);
    assert!(
        candidates[0]
            .build
            .contains("npm run tauri -- build --bundles app --config ")
    );
}

#[test]
fn prebuild_hook_is_not_bypassed_when_tauri_2_is_detected() {
    let files = BTreeMap::from([
        (
            "package.json".into(),
            r#"{"scripts":{"prebuild":"node prepare.js","build":"tauri build","tauri":"tauri"}}"#
                .into(),
        ),
        (
            "src-tauri/tauri.conf.json".into(),
            r#"{"$schema":"https://schema.tauri.app/config/2"}"#.into(),
        ),
    ]);
    let candidates = manifests::candidates(&files, &[]);
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
    );
}

#[test]
fn postbuild_hook_is_not_bypassed_when_tauri_2_is_detected() {
    let files = BTreeMap::from([
        (
            "package.json".into(),
            r#"{"scripts":{"postbuild":"node finish.js","build":"tauri build","tauri":"tauri"}}"#
                .into(),
        ),
        (
            "src-tauri/tauri.conf.json".into(),
            r#"{"$schema":"https://schema.tauri.app/config/2"}"#.into(),
        ),
    ]);
    let candidates = manifests::candidates(&files, &[]);
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
    );
}
