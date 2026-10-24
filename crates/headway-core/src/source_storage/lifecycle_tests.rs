use super::*;

#[tokio::test]
async fn failed_verification_also_removes_the_temporary_checkout() {
    // Given a temporary source build whose post-install verifier rejects the payload.
    let f = Fixture::new(false, true, BUILD).await;
    let app = with_github(f.app.clone(), |mut github| {
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.verify_command = Some("exit 17".into());
        github
    });
    f.hw.replace(app).unwrap();
    // When verification triggers installation rollback.
    assert!(f.update().await.is_err());
    // Then neither the temporary checkout nor an unverified install is retained.
    assert!(!f.checkout().exists());
    assert_eq!(fs::read_dir(&f.base).unwrap().count(), 0);
    assert!(
        f.hw.store()
            .state()
            .unwrap()
            .app("tool")
            .installed
            .is_none()
    );
}

#[tokio::test]
async fn retained_checkout_survives_a_build_failure_for_later_reuse() {
    // Given keep=true and a real source build that exits unsuccessfully.
    let f = Fixture::new(
        true,
        true,
        "printf '%s' \"$PWD\" > \"$TEST_WORKDIR\"; exit 19",
    )
    .await;
    // When the build fails.
    assert!(f.update().await.is_err());
    // Then retention preserves its checkout while installed state remains absent.
    assert!(f.checkout().join(".git").is_dir());
    assert!(f.hw.source_storage_status("tool").unwrap().exists);
    assert!(
        f.hw.store()
            .state()
            .unwrap()
            .app("tool")
            .installed
            .is_none()
    );
}

#[tokio::test]
async fn disabled_retention_preserves_a_previously_kept_checkout() {
    // Given an existing owned checkout with cached build output.
    let f = Fixture::new(true, true, BUILD).await;
    f.update().await.unwrap();
    let old = f.checkout();
    // When future builds switch to temporary mode at the same base.
    f.hw.replace(f.with_storage(SourceStorage {
        keep: false,
        root: Some(f.base.display().to_string()),
    }))
    .unwrap();
    f.update().await.unwrap();
    // Then the new checkout is cleaned while existing retained data is untouched.
    assert_ne!(f.checkout(), old);
    assert!(!f.checkout().exists());
    assert_eq!(fs::read(old.join("target/cache")).unwrap(), b"retained\n");
    let status = f.hw.source_storage_status("tool").unwrap();
    assert!(!status.keep && status.exists);
}

#[tokio::test]
async fn plans_show_selected_storage_and_temporary_cleanup() {
    // Given a source build configured for a temporary custom location.
    let f = Fixture::new(false, true, BUILD).await;
    // When planning an explicit update without creating a source directory.
    let plan =
        f.hw.plan("tool", &Target::Ref(f.sha.clone()))
            .await
            .unwrap();
    // Then the plan describes the selected base and cleanup lifecycle truthfully.
    assert!(plan.steps[0].contains("custom sources"));
    assert!(
        plan.steps
            .iter()
            .any(|step| step.contains("temporary source checkout"))
    );
    assert!(!f.base.exists());
}

#[tokio::test]
async fn source_status_uses_the_same_update_lock() {
    // Given an in-progress update that owns the app lock.
    let f = Fixture::new(true, true, BUILD).await;
    let _lock = AppLock::acquire(&f.hw.paths().app_lock("tool"), "tool").unwrap();
    // When inspecting storage during that update.
    let result = f.hw.source_storage_status("tool");
    // Then no stale status is read or storage created.
    assert!(matches!(result, Err(Error::Busy(_))));
    assert!(!f.base.exists());
}
