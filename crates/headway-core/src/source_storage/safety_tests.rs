use super::*;

#[test]
fn source_storage_defaults_keep_existing_caches_and_reject_traversal() {
    // Given old serialized SourceInstall data without a storage block.
    let old: SourceInstall = serde_json::from_value(serde_json::json!({"build":"make"})).unwrap();
    // When interpreting legacy defaults or unsafe configured roots.
    assert_eq!(
        old.storage,
        SourceStorage {
            keep: true,
            root: None
        }
    );
    for root in ["", "relative", "/tmp/../escape", "~/../escape"] {
        // Then invalid syntax cannot resolve a writable managed child.
        assert!(
            SourceStorage {
                keep: true,
                root: Some(root.into())
            }
            .validate()
            .is_err()
        );
    }
}

#[tokio::test]
async fn unknown_nonempty_custom_child_is_never_reused_or_purged() {
    // Given unrelated user data where an app's checkout child would go.
    let f = Fixture::new(true, true, BUILD).await;
    fs::create_dir_all(f.base.join("tool")).unwrap();
    let user = f.base.join("tool/user-data");
    fs::write(&user, "precious").unwrap();
    // When update or purge attempts to use that unowned nonempty directory.
    assert!(f.update().await.is_err());
    assert!(f.hw.remove("tool", true).is_err());
    // Then no user data or tracking config is removed.
    assert_eq!(fs::read(user).unwrap(), b"precious");
    assert!(f.hw.get("tool").is_ok());
}

#[tokio::test]
async fn purge_removes_only_owned_clone_and_preserves_custom_base() {
    // Given a retained clone and unrelated data alongside it.
    let f = Fixture::new(true, true, BUILD).await;
    f.update().await.unwrap();
    let clone = f.checkout();
    fs::write(f.base.join("user-file"), "keep").unwrap();
    // When the app is removed with explicit cache purge.
    f.hw.remove("tool", true).unwrap();
    // Then the clone is gone, while its base and unrelated file remain.
    assert!(!clone.exists());
    assert_eq!(fs::read(f.base.join("user-file")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(&f.base).unwrap().count(), 1);
}

#[tokio::test]
async fn changing_roots_does_not_delete_the_old_owned_clone() {
    // Given a successful retained clone at the first custom root.
    let f = Fixture::new(true, true, BUILD).await;
    f.update().await.unwrap();
    let old = f.checkout();
    let next = f.root.path().join("next source root");
    // When selecting another root, building, and purging that current location.
    f.hw.replace(f.with_storage(SourceStorage {
        keep: true,
        root: Some(next.display().to_string()),
    }))
    .unwrap();
    assert!(old.exists());
    f.update().await.unwrap();
    let current = f.checkout();
    f.hw.remove("tool", true).unwrap();
    // Then the old clone remains untouched and the new base is preserved empty.
    assert!(old.join(".git").is_dir());
    assert!(!current.exists());
    assert!(next.is_dir());
}

#[tokio::test]
async fn symlink_root_is_rejected() {
    // Given a selected root that is redirected to another user directory.
    let f = Fixture::new(true, true, BUILD).await;
    let outside = f.root.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, &f.base).unwrap();
    // When resolving or updating source storage.
    assert!(f.hw.source_storage_status("tool").is_err());
    assert!(f.update().await.is_err());
    // Then the symlink target stays untouched.
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[tokio::test]
async fn symlink_checkout_child_is_never_followed_or_purged() {
    // Given a child path redirected to another directory.
    let f = Fixture::new(true, true, BUILD).await;
    let outside = f.root.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep"), "keep").unwrap();
    fs::create_dir_all(&f.base).unwrap();
    std::os::unix::fs::symlink(&outside, f.base.join("tool")).unwrap();
    // When update and purge encounter that child.
    assert!(f.update().await.is_err());
    assert!(f.hw.remove("tool", true).is_err());
    // Then data at the link target survives.
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");
}

#[tokio::test]
async fn symlink_ownership_record_is_rejected_without_touching_its_target() {
    // Given a retained clone whose record was replaced by a symlink.
    let f = Fixture::new(true, true, BUILD).await;
    f.update().await.unwrap();
    let marker = f.base.join(".tool.headway-source.json");
    fs::remove_file(&marker).unwrap();
    let outside = f.root.path().join("outside-file");
    fs::write(&outside, "keep").unwrap();
    std::os::unix::fs::symlink(&outside, &marker).unwrap();
    // When the source location is inspected, updated, or purged.
    assert!(f.hw.source_storage_status("tool").is_err());
    assert!(f.update().await.is_err());
    assert!(f.hw.remove("tool", true).is_err());
    // Then the external file and clone remain intact.
    assert_eq!(fs::read(outside).unwrap(), b"keep");
    assert!(f.checkout().join(".git").exists());
}

#[tokio::test]
async fn another_headway_store_cannot_claim_an_existing_owned_clone() {
    // Given a custom root already owned by one store.
    let f = Fixture::new(true, true, BUILD).await;
    f.update().await.unwrap();
    let other = Headway::builder()
        .paths(Paths::under(f.root.path().join("another-home")))
        .build()
        .unwrap();
    other.add(f.app.clone()).unwrap();
    // When a separate store tries to inspect or purge the same app child.
    assert!(other.source_storage_status("tool").is_err());
    assert!(other.remove("tool", true).is_err());
    // Then the original store retains its checkout and configuration.
    assert!(f.checkout().join(".git").is_dir());
    assert!(f.hw.source_storage_status("tool").unwrap().exists);
}
