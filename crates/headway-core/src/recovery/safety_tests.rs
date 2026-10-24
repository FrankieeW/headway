use super::*;

#[tokio::test]
async fn missing_metadata_and_outside_payload_paths_are_rejected() {
    // Given a cached snapshot whose metadata points outside its directory.
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.place("new").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    let dir = PathBuf::from(&backup.artifact_path)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let metadata = dir.join("metadata.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    document["entry"]["artifact_path"] = serde_json::json!(f.src);
    fs::write(&metadata, serde_json::to_vec(&document).unwrap()).unwrap();
    // When listing tampered metadata, and then a missing manifest.
    assert!(f.hw.list_backups("tool").is_err());
    fs::remove_file(metadata).unwrap();
    assert!(
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    // Then the current payload survives both invalid inputs.
    assert_eq!(fs::read(&f.dest).unwrap(), b"new");
}

#[tokio::test]
async fn recovery_honors_the_update_lock() {
    // Given an app already locked by an updater.
    let f = Fixture::new(None);
    let _lock = crate::lock::AppLock::acquire(&f.hw.paths().app_lock("tool"), "tool").unwrap();
    // When attempting recovery while that update owns the destination.
    let error =
        f.hw.restore_backup("tool", "backup", &NullSink, &CancellationToken::new())
            .await
            .unwrap_err();
    // Then recovery reports contention without touching files.
    assert!(matches!(error, crate::Error::Busy(_)));
    assert!(!f.dest.exists());
}

#[tokio::test]
async fn verifier_receives_candidate_fields_without_github_token() {
    // Given an explicit token-bearing environment and candidate identity.
    let f = Fixture::new(Some(
        "test \"$HEADWAY_REF\" = revision && test \"$HEADWAY_SHA\" = fullsha && test \"$HEADWAY_VERSION\" = v1 && test -z \"${HEADWAY_GITHUB_TOKEN:-}\" && test \"$(cat \"$HEADWAY_INSTALLED_PATH\")\" = payload",
    ));
    fs::write(&f.src, "payload").unwrap();
    let candidate = InstalledRef {
        id: "revision".into(),
        sha: Some("fullsha".into()),
        version: Some("v1".into()),
        installed_at: Utc::now(),
        path: None,
    };
    let env = ShellEnv::current().with_var("HEADWAY_GITHUB_TOKEN", "test-sentinel");
    let log = RunLog::create(&f.hw.paths().app_log_dir("tool"), "tool", &NullSink).unwrap();
    let cancel = CancellationToken::new();
    // When copying and verifying the candidate.
    Replacement {
        app: &f.app,
        paths: f.hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&f.src, Some(&candidate), &f.dest)
    .await
    .unwrap();
    // Then the shell checks succeeded against the installed bytes and clean environment.
    assert_eq!(fs::read(&f.dest).unwrap(), b"payload");
}

#[tokio::test]
async fn unsigned_bundle_is_accepted_without_opted_in_verification() {
    // Given an unsigned third-party bundle and no verifier.
    let f = Fixture::new(None);
    let src = f.dir.path().join("build/Demo.app");
    let dest = f.dir.path().join("apps/Demo.app");
    fs::create_dir_all(src.join("Contents/MacOS")).unwrap();
    fs::write(src.join("Contents/MacOS/Demo"), "unsigned").unwrap();
    let env = ShellEnv::current();
    let log = RunLog::create(&f.hw.paths().app_log_dir("tool"), "tool", &NullSink).unwrap();
    let cancel = CancellationToken::new();
    // When installing the bundle with normal recovery enabled.
    Replacement {
        app: &f.app,
        paths: f.hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&src, None, &dest)
    .await
    .unwrap();
    // Then unsigned third-party apps are not blanket-rejected.
    assert_eq!(
        fs::read(dest.join("Contents/MacOS/Demo")).unwrap(),
        b"unsigned"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn configured_bundle_verification_checks_native_signature_and_rolls_back() {
    // Given an unsigned candidate and a prior bundle.
    let f = Fixture::new(Some("true"));
    let src = f.dir.path().join("build/Demo.app");
    let dest = f.dir.path().join("apps/Demo.app");
    for path in [&src, &dest] {
        fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
    }
    fs::write(src.join("Contents/MacOS/Demo"), "unsigned-new").unwrap();
    fs::write(dest.join("Contents/MacOS/Demo"), "old").unwrap();
    let env = ShellEnv::current();
    let log = RunLog::create(&f.hw.paths().app_log_dir("tool"), "tool", &NullSink).unwrap();
    let cancel = CancellationToken::new();
    // When installing with user verification enabled.
    let result = Replacement {
        app: &f.app,
        paths: f.hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&src, None, &dest)
    .await;
    // Then native codesign rejects the candidate and the previous bundle returns.
    assert!(
        matches!(result, Err(crate::Error::CommandFailed { command, .. }) if command.contains("codesign"))
    );
    assert_eq!(fs::read(dest.join("Contents/MacOS/Demo")).unwrap(), b"old");
}

#[test]
fn plain_directory_cannot_replace_a_bundle() {
    // Given a bundle and an unrelated plain directory as candidate.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("plain");
    let dest = dir.path().join("Demo.app");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("old"), "keep").unwrap();
    // When invoking the shared installer.
    assert!(crate::install::install(&src, &dest).is_err());
    // Then the bundle-only replacement guard preserves the old bundle.
    assert_eq!(fs::read(dest.join("old")).unwrap(), b"keep");
}

#[tokio::test]
async fn failed_rollback_preserves_rescue_payload_and_clears_installed_certainty() {
    // Given a verifier that replaces the destination with a plain directory.
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    let app = with_github(f.app.clone(), |mut github| {
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.verify_command = Some(
            "rm \"$HEADWAY_INSTALLED_PATH\"; mkdir \"$HEADWAY_INSTALLED_PATH\"; exit 1".into(),
        );
        github
    });
    fs::write(&f.src, "new").unwrap();
    let env = ShellEnv::current();
    let log = RunLog::create(&f.hw.paths().app_log_dir("tool"), "tool", &NullSink).unwrap();
    let cancel = CancellationToken::new();
    // When rollback is refused by the directory guard after failed verification.
    let error = Replacement {
        app: &app,
        paths: f.hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&f.src, None, &f.dest)
    .await
    .unwrap_err();
    // Then the only old payload remains available for rescue and state is unknown.
    assert!(error.to_string().contains("rollback failed"));
    assert!(f.installed().is_none());
    let pending = fs::read_dir(f.hw.paths().cache_dir.join("backups/tool"))
        .unwrap()
        .map(|item| item.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".pending-")
        })
        .unwrap();
    assert_eq!(fs::read(pending.join("payload/tool")).unwrap(), b"old");
}
