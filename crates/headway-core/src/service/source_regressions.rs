use super::*;

#[tokio::test]
async fn latest_only_scripts_never_treat_the_remote_version_as_installed() {
    // Given an updater with a latest version but no installed-version observation.
    let state = tempfile::tempdir().unwrap();
    let runs = state.path().join("runs");
    let (_root, hw) =
        headway(ShellEnv::from_vars([]).with_var("HEADWAY_TEST_RUNS", runs.display().to_string()));
    hw.add(app(Source::Script(ScriptSource {
        update_command: "echo run >> \"$HEADWAY_TEST_RUNS\"".into(),
        version_command: None,
        latest_command: Some("echo 1.2.3".into()),
    })))
    .unwrap();
    hw.update(
        "tool",
        &Target::Latest,
        &NullSink,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    // When explicitly invoking the updater again.
    let result = hw
        .update(
            "tool",
            &Target::Latest,
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    // Then both requests execute, and no remote candidate becomes an installed fact.
    assert_eq!(std::fs::read_to_string(runs).unwrap().lines().count(), 2);
    assert!(result.to.version.is_none());
    let view = hw.get("tool").unwrap();
    assert!(view.installed.is_none());
    assert_eq!(view.status, AppStatus::Unknown);
    assert!(!hw.plan("tool", &Target::Latest).await.unwrap().up_to_date);
}

#[tokio::test]
async fn explicit_npm_versions_do_not_depend_on_a_deleted_dist_tag() {
    // Given a package whose previously followed channel was removed.
    let state = tempfile::tempdir().unwrap();
    let version = state.path().join("version");
    std::fs::write(&version, "1.2.0").unwrap();
    let tools = fake_npm();
    let (_root, hw) = headway(
        tools
            .env()
            .with_var("HEADWAY_TEST_VERSION", version.display().to_string()),
    );
    hw.add(app(Source::Npm(NpmSource {
        package: "@acme/tool".into(),
        dist_tag: "removed".into(),
    })))
    .unwrap();
    let target = Target::Ref("1.0.0".into());
    let plan = hw.plan("tool", &target).await.unwrap();
    assert_eq!(plan.to.id, "1.0.0");
    assert_eq!(std::fs::read_to_string(&version).unwrap(), "1.2.0");
    // When explicitly rolling back to an existing version.
    let result = hw
        .update("tool", &target, &NullSink, &CancellationToken::new())
        .await
        .unwrap();
    // Then rollback succeeds even though a normal channel check cannot resolve.
    assert_eq!(result.to.id, "1.0.0");
    assert_eq!(std::fs::read_to_string(version).unwrap(), "1.0.0");
    assert_eq!(hw.get("tool").unwrap().installed.unwrap().id, "1.0.0");
}

#[tokio::test]
async fn cancellation_interrupts_a_confirmed_running_version_probe() {
    // Given a version probe that announces readiness before blocking.
    let state = tempfile::tempdir().unwrap();
    let started = state.path().join("started");
    let updated = state.path().join("updated");
    let (_root, hw) = headway(
        ShellEnv::from_vars([])
            .with_var("HEADWAY_TEST_STARTED", started.display().to_string())
            .with_var("HEADWAY_TEST_UPDATED", updated.display().to_string()),
    );
    hw.add(app(Source::Script(ScriptSource {
        update_command: "echo ran > \"$HEADWAY_TEST_UPDATED\"".into(),
        version_command: Some("echo ready > \"$HEADWAY_TEST_STARTED\"; exec /bin/sleep 30".into()),
        latest_command: Some("echo 1.2.3".into()),
    })))
    .unwrap();
    let cancel = CancellationToken::new();
    let cancel_when_started = async {
        while !started.exists() {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
    };
    // When cancellation arrives during the probe rather than during installation.
    let (outcome, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            hw.update("tool", &Target::Latest, &NullSink, &cancel),
            cancel_when_started
        )
    })
    .await
    .unwrap();
    // Then cancellation is prompt and truthful, and no updater or lock survives it.
    assert_eq!(outcome.unwrap_err().code(), "cancelled");
    assert!(!updated.exists());
    assert!(!hw.get("tool").unwrap().busy);
}

#[tokio::test]
async fn cancelling_optional_refresh_keeps_the_confirmed_successful_installation() {
    // Given a successful updater followed by a refresh probe that blocks.
    let state = tempfile::tempdir().unwrap();
    let started = state.path().join("refresh-started");
    let (_root, hw) = headway(
        ShellEnv::from_vars([]).with_var("HEADWAY_TEST_ROOT", state.path().display().to_string()),
    );
    hw.add(app(Source::Script(ScriptSource {
        update_command: "touch \"$HEADWAY_TEST_ROOT/updated\"".into(),
        version_command: Some(
            r#"
if [ -f "$HEADWAY_TEST_ROOT/observed" ]; then
  echo ready > "$HEADWAY_TEST_ROOT/refresh-started"
  exec /bin/sleep 30
elif [ -f "$HEADWAY_TEST_ROOT/updated" ]; then
  touch "$HEADWAY_TEST_ROOT/observed"
  echo 2.0.0
else echo 1.0.0
fi"#
            .into(),
        ),
        latest_command: Some("echo 2.0.0".into()),
    })))
    .unwrap();
    let cancel = CancellationToken::new();
    let cancel_when_started = async {
        while !started.exists() {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
    };
    // When cancelling the optional refresh after installation was verified.
    let (outcome, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            hw.update("tool", &Target::Latest, &NullSink, &cancel),
            cancel_when_started
        )
    })
    .await
    .unwrap();
    // Then the confirmed installation is retained and its lock is promptly released.
    assert_eq!(outcome.unwrap().to.id, "2.0.0");
    let view = hw.get("tool").unwrap();
    assert_eq!(view.installed.unwrap().id, "2.0.0");
    assert!(!view.busy);
    assert!(view.last_error.is_none());
}
