use super::*;

#[test]
fn standard_tauri_2_build_recommends_local_app() {
    // Given a standard Tauri 2 build and a pnpm lockfile.
    let files = BTreeMap::from([
        ("package.json".into(), r#"{"packageManager":"pnpm@10","scripts":{"build":"pnpm tauri build","tauri":"tauri"}}"#.into()),
        ("src-tauri/tauri.conf.json".into(), r#"{"$schema":"https://schema.tauri.app/config/2","bundle":{"createUpdaterArtifacts":true}}"#.into()),
    ]);
    let entries = vec![ContentEntry {
        name: "pnpm-lock.yaml".into(),
        kind: "file".into(),
    }];
    // When detecting and ranking build candidates.
    let mut candidates = manifests::candidates(&files, &entries);
    prioritize(&mut candidates, false);
    // Then the local app command applies signing only to the Tauri step.
    let local = &candidates[0];
    assert_eq!(local.id, manifests::TAURI_LOCAL_ID);
    assert!(local.recommended);
    assert_eq!(
        local.build,
        "pnpm install --frozen-lockfile && if [ -z \"${APPLE_SIGNING_IDENTITY:-}\" ]; then export APPLE_SIGNING_IDENTITY=-; unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_KEY_PATH APPLE_API_ISSUER; fi && pnpm run tauri build --bundles app --config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'"
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
    // Given unknown-version and Tauri 1 configurations with only a nested identifier.
    for config in [
        r#"{"tauri":{"bundle":{"active":true}}}"#,
        r#"{"$schema":"../node_modules/@tauri-apps/cli/schema.json","tauri":{"bundle":{"identifier":"com.example.app"}}}"#,
    ] {
        let files = BTreeMap::from([
            (
                "package.json".into(),
                r#"{"scripts":{"tauri":"tauri"}}"#.into(),
            ),
            ("src-tauri/tauri.conf.json".into(), config.into()),
        ]);
        // When detecting manifest commands.
        let candidates = manifests::candidates(&files, &[]);
        // Then neither local signing nor an updater override is introduced.
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.id != manifests::TAURI_LOCAL_ID)
        );
        assert_eq!(
            candidates[0].build,
            "npm install && npm run tauri -- build --bundles app"
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
        );
    }
}

#[test]
fn custom_build_script_is_not_bypassed_when_tauri_2_is_detected() {
    // Given a customized script that runs Tauri after a preparation step.
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
    // When detecting manifest commands.
    let candidates = manifests::candidates(&files, &[]);
    // Then the custom build remains available without a local override.
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
    // Given a Tauri 2 project using npm.
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
    // When detecting manifest commands.
    let candidates = manifests::candidates(&files, &[]);
    // Then npm forwards all build flags to Tauri after its argument separator.
    assert_eq!(
        candidates[0].build,
        "npm install && if [ -z \"${APPLE_SIGNING_IDENTITY:-}\" ]; then export APPLE_SIGNING_IDENTITY=-; unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_KEY_PATH APPLE_API_ISSUER; fi && npm run tauri -- build --bundles app --config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'"
    );
}

#[test]
fn prebuild_hook_is_not_bypassed_when_tauri_2_is_detected() {
    // Given a Tauri build with a prebuild hook.
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
    // When detecting manifest commands.
    let candidates = manifests::candidates(&files, &[]);
    // Then local overrides do not bypass the hook.
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
    );
}

#[test]
fn postbuild_hook_is_not_bypassed_when_tauri_2_is_detected() {
    // Given a Tauri build with a postbuild hook.
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
    // When detecting manifest commands.
    let candidates = manifests::candidates(&files, &[]);
    // Then local overrides do not bypass the hook.
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.build.contains("createUpdaterArtifacts"))
    );
}

#[test]
fn create_tauri_app_frontend_builds_recommend_local_app() {
    // Given create-tauri-app layouts, including a frontend prebuild hook.
    for scripts in [
        r#"{"build":"tsc && vite build","tauri":"tauri"}"#,
        r#"{"build":"vite build","tauri":"tauri"}"#,
        r#"{"build":"vite build","prebuild":"node gen.js","tauri":"tauri"}"#,
    ] {
        let files = BTreeMap::from([
            ("package.json".into(), format!(r#"{{"scripts":{scripts}}}"#)),
            (
                "src-tauri/tauri.conf.json".into(),
                r#"{"$schema":"https://schema.tauri.app/config/2","build":{"beforeBuildCommand":"pnpm run build"}}"#.into(),
            ),
        ]);
        let entries = vec![ContentEntry {
            name: "pnpm-lock.yaml".into(),
            kind: "file".into(),
        }];
        // When detecting and ranking build candidates.
        let mut candidates = manifests::candidates(&files, &entries);
        prioritize(&mut candidates, false);
        // Then an invoked frontend build preserves hooks and allows the local app recommendation.
        assert_eq!(candidates[0].id, manifests::TAURI_LOCAL_ID, "{scripts}");
        assert!(candidates[0].recommended);
    }
}

#[test]
fn tauri_2_identifier_detects_local_app_without_remote_schema() {
    // Given Tauri 2 configs with the tauri init schema or no schema at all.
    for config in [
        r#"{"$schema":"../node_modules/@tauri-apps/cli/config.schema.json","identifier":"com.example.app"}"#,
        r#"{"identifier":"com.example.app"}"#,
    ] {
        let files = BTreeMap::from([
            (
                "package.json".into(),
                r#"{"scripts":{"tauri":"tauri"}}"#.into(),
            ),
            ("src-tauri/tauri.conf.json".into(), config.into()),
        ]);
        // When detecting manifest commands.
        let candidates = manifests::candidates(&files, &[]);
        // Then a top-level identifier is sufficient evidence for a local Tauri 2 app.
        assert_eq!(candidates[0].id, manifests::TAURI_LOCAL_ID, "{config}");
    }
}

#[test]
fn known_headway_self_build_stays_first() {
    // Given Headway's manifests and its known self-build preset.
    let files = BTreeMap::from([
        ("package.json".into(), r#"{"packageManager":"pnpm@12.14.0","scripts":{"build":"tauri build","tauri":"tauri"}}"#.into()),
        ("src-tauri/tauri.conf.json".into(), r#"{"$schema":"https://schema.tauri.app/config/2","identifier":"wang.frankie.headway"}"#.into()),
    ]);
    let preset = crate::presets::find("headway-app").unwrap();
    let mut candidates = manifests::candidates(&files, &[]);
    candidates.push(BuildCandidate {
        id: format!("preset:{}", preset.id),
        source: BuildCandidateSource::Preset,
        file: "scripts/build-headway.sh".into(),
        line: None,
        build: preset.build.into(),
        workdir: None,
        artifact: preset.artifact.map(str::to_owned),
        kind: preset.kind,
        reason: format!(
            "Detected {} source layout; review the preset before saving.",
            preset.label
        ),
        recommended: false,
    });
    // When ranking candidates for the known Headway repository.
    prioritize(&mut candidates, true);
    // Then the self-build stays recommended and the local app remains an alternative.
    assert_eq!(candidates[0].id, "preset:headway-app");
    assert!(candidates[0].recommended);
    let local = candidates
        .iter()
        .find(|candidate| candidate.id == manifests::TAURI_LOCAL_ID)
        .unwrap();
    assert!(!local.recommended);
}

#[test]
fn plain_tauri_wrappers_require_no_lifecycle_hooks() {
    // Given ordinary wrappers and optional npm lifecycle hooks.
    for build in [
        "tauri build",
        "pnpm tauri build",
        "pnpm run tauri build",
        "npm run tauri -- build",
        "npm run tauri build",
        "yarn tauri build",
        "yarn run tauri build",
        "bun tauri build",
        "bun run tauri build",
        "tauri build --verbose",
        "node prepare.js && tauri build",
    ] {
        for hook in [None, Some("prebuild"), Some("postbuild")] {
            let mut scripts = serde_json::json!({"build": build, "tauri": "tauri"});
            if let Some(hook) = hook {
                scripts[hook] = "node hook.js".into();
            }
            let files = BTreeMap::from([
                (
                    "package.json".into(),
                    serde_json::json!({"scripts": scripts}).to_string(),
                ),
                (
                    "src-tauri/tauri.conf.json".into(),
                    r#"{"identifier":"com.example.app"}"#.into(),
                ),
            ]);
            // When detecting local candidates.
            let candidates = manifests::candidates(&files, &[]);
            // Then only plain wrappers without bypassed hooks qualify.
            let expected = hook.is_none() && !build.contains("--verbose") && !build.contains("&&");
            assert_eq!(
                candidates.iter().any(|c| c.id == manifests::TAURI_LOCAL_ID),
                expected,
                "{build}, {hook:?}"
            );
        }
    }
}

#[test]
fn frontend_build_requires_matching_manager_build_invocation() {
    // Given a frontend or release build, with and without lifecycle hooks.
    for manager in ["pnpm", "npm", "yarn", "bun"] {
        for command in [
            None,
            Some("pnpm build"),
            Some("pnpm run build"),
            Some("npm run build"),
            Some("yarn build"),
            Some("yarn run build"),
            Some("bun run build"),
            Some("bun build"),
            Some("pnpm run build:desktop"),
            Some("vite build"),
            Some("node scripts/release.js"),
            Some("pnpm run rebuild"),
            Some("pnpm run build && node finish.js"),
        ] {
            for build in ["vite build", "node scripts/release.js"] {
                let files = BTreeMap::from([
                    ("package.json".into(), serde_json::json!({
                        "packageManager": format!("{manager}@10"),
                        "scripts": {"build": build, "tauri": "tauri", "prebuild": "node gen.js", "postbuild": "node finish.js"}
                    }).to_string()),
                    ("src-tauri/tauri.conf.json".into(), serde_json::json!({
                        "identifier": "com.example.app", "build": {"beforeBuildCommand": command}
                    }).to_string()),
                ]);
                // When detecting and ranking candidates.
                let mut candidates = manifests::candidates(&files, &[]);
                prioritize(&mut candidates, false);
                // Then the build script and its hooks must actually be invoked by Tauri.
                let expected = command.is_some_and(|command| {
                    command == format!("{manager} run build")
                        || (manager != "npm" && command == format!("{manager} build"))
                });
                assert_eq!(
                    candidates[0].id == manifests::TAURI_LOCAL_ID,
                    expected,
                    "{manager}, {command:?}, {build}"
                );
            }
        }
    }
}
