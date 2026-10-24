use super::*;

fn repoint(f: &mut Fixture, repo: &str) {
    f.app = with_github(f.app.clone(), |mut github| {
        github.repo = repo.into();
        github
    });
    f.hw.replace(f.app.clone()).unwrap();
}

#[tokio::test]
async fn repointed_repository_filters_foreign_backups_without_blocking_new_payloads() {
    // Given source A backups and an existing payload at the same destination.
    let mut f = Fixture::new(None);
    f.place("a-old").await.unwrap();
    f.place("a-current").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    // When the subscription now tracks source B at that same path.
    repoint(&mut f, "other/tool");
    // Then A's ref is never exposed or attributed to B, including explicit restore.
    assert!(f.hw.list_backups("tool").unwrap().is_empty());
    assert!(
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(f.installed().is_none());
    assert_eq!(fs::read(&f.dest).unwrap(), b"a-current");
    for revision in ["b-first", "b-second", "b-third"] {
        f.place(revision).await.unwrap();
    }
    let backups = f.hw.list_backups("tool").unwrap();
    assert_eq!(backups.len(), 2);
    assert!(
        backups
            .iter()
            .all(|entry| entry.installed.as_ref().unwrap().id.starts_with("b-"))
    );
    assert_eq!(
        fs::read_dir(f.hw.paths().cache_dir.join("backups/tool"))
            .unwrap()
            .count(),
        2
    );
    println!(
        "identity_repoint_observation={}",
        serde_json::json!({"installed":f.installed(),"backups":backups})
    );
}

#[tokio::test]
async fn reused_id_cannot_restore_previous_repository_revision() {
    // Given a removed subscription whose cache was intentionally retained.
    let mut f = Fixture::new(None);
    f.place("a-old").await.unwrap();
    f.place("a-current").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    f.hw.remove("tool", false).unwrap();
    f.app = with_github(f.app.clone(), |mut github| {
        github.repo = "other/tool".into();
        github
    });
    // When reusing its ID and destination for another repository.
    f.hw.add(f.app.clone()).unwrap();
    // Then old identity cannot leak into the new subscription's installed state.
    assert!(f.hw.list_backups("tool").unwrap().is_empty());
    assert!(
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(f.installed().is_none());
    assert_eq!(fs::read(&f.dest).unwrap(), b"a-current");
}

#[tokio::test]
async fn purge_removes_backups_before_an_id_is_reused() {
    // Given a subscription with retained payloads.
    let mut f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.place("current").await.unwrap();
    let root = f.hw.paths().cache_dir.join("backups/tool");
    assert!(root.exists());
    // When explicitly removing it with purge.
    f.hw.remove("tool", true).unwrap();
    // Then its recovery cache is removed and reuse starts empty.
    assert!(!root.exists());
    f.app = with_github(f.app.clone(), |mut github| {
        github.repo = "other/tool".into();
        github
    });
    f.hw.add(f.app.clone()).unwrap();
    assert!(f.hw.list_backups("tool").unwrap().is_empty());
    assert!(f.installed().is_none());
}

#[tokio::test]
async fn track_mode_changes_invalidate_revision_evidence() {
    // Given commit-tracking backup metadata.
    let mut f = Fixture::new(None);
    f.place("old-sha").await.unwrap();
    f.place("current-sha").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    // When the same repo switches to release tracking.
    f.app = with_github(f.app.clone(), |mut github| {
        github.track = crate::Track::Release {
            prerelease: false,
            tag_filter: None,
        };
        github
    });
    f.hw.replace(f.app.clone()).unwrap();
    // Then a commit id cannot be restored as an installed release id.
    assert!(f.hw.list_backups("tool").unwrap().is_empty());
    assert!(
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(f.installed().is_none());
}

#[tokio::test]
async fn repository_case_branch_build_and_verification_edits_preserve_backups() {
    // Given a known repo's retained revision.
    let mut f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.place("current").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    // When only equivalent repo casing and nonidentity installation settings change.
    f.app = with_github(f.app.clone(), |mut github| {
        github.repo = "OWNER/TOOL".into();
        github.track = crate::Track::Commit {
            branch: Some("next".into()),
        };
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.build = "different-build".into();
        source.verify_command = Some("true".into());
        github
    });
    f.hw.replace(f.app.clone()).unwrap();
    // Then recovery still identifies the same canonical repository and mode.
    assert_eq!(f.hw.list_backups("tool").unwrap().len(), 1);
    let outcome =
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .unwrap();
    assert_eq!(outcome.installed.unwrap().id, "old");
    assert_eq!(fs::read(&f.dest).unwrap(), b"old");
}

#[tokio::test]
async fn foreign_identity_does_not_hide_corrupt_payloads() {
    // Given a foreign backup with damaged payload bytes.
    let mut f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.place("current").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    fs::write(&backup.artifact_path, "corrupt").unwrap();
    repoint(&mut f, "other/tool");
    // When the cache is listed for the new identity.
    let error = f.hw.list_backups("tool").unwrap_err();
    // Then corruption is surfaced rather than silently filtered as an unrelated backup.
    assert!(error.to_string().contains("corrupt"));
}

#[tokio::test]
async fn unbound_legacy_manifest_is_rejected_without_guessing_a_repository() {
    // Given a pre-identity manifest whose origin cannot be established.
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.place("current").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    let metadata = PathBuf::from(&backup.artifact_path)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("metadata.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    manifest.as_object_mut().unwrap().remove("source_identity");
    fs::write(&metadata, serde_json::to_vec(&manifest).unwrap()).unwrap();
    // When listing or explicitly restoring it.
    assert!(matches!(
        f.hw.list_backups("tool"),
        Err(crate::Error::Parse { .. })
    ));
    assert!(
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    // Then the known current ref and bytes are not replaced with unbound evidence.
    assert_eq!(f.installed().unwrap().id, "current");
    assert_eq!(fs::read(&f.dest).unwrap(), b"current");
}

#[tokio::test]
async fn unsafe_backup_root_blocks_purge_before_removing_tracking() {
    // Given a backup-root symlink pointing outside the managed cache.
    let f = Fixture::new(None);
    let outside = f.dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep"), "keep").unwrap();
    let root = f.hw.paths().cache_dir.join("backups");
    fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("tool")).unwrap();
    // When purging the subscription.
    assert!(f.hw.remove("tool", true).is_err());
    // Then neither external files nor the tracked subscription are removed.
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");
    assert!(f.hw.get("tool").is_ok());
}
