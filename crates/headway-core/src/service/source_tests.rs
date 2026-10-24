use super::*;
use crate::events::NullSink;
use crate::source::{BrewSource, NpmSource, ScriptSource};
use crate::testing::FakeTools;

#[path = "source_regressions.rs"]
mod regressions;

struct UnavailableSecrets;

impl SecretStore for UnavailableSecrets {
    fn get(&self) -> Result<Option<String>> {
        Err(Error::Keychain("unavailable in this test".into()))
    }
    fn set(&self, _: &str) -> Result<()> {
        Err(Error::Keychain("unavailable in this test".into()))
    }
    fn delete(&self) -> Result<bool> {
        Err(Error::Keychain("unavailable in this test".into()))
    }
}

fn headway(env: ShellEnv) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .secrets(Arc::new(UnavailableSecrets))
        .env(env)
        .api("http://127.0.0.1:9")
        .build()
        .unwrap();
    (dir, hw)
}

fn app(source: Source) -> TrackedApp {
    TrackedApp {
        id: "tool".into(),
        name: "Tool".into(),
        kind: crate::AppKind::Cli,
        auto_update: false,
        update_policy: crate::UpdatePolicy::default(),
        source,
    }
}

fn npm() -> Source {
    Source::Npm(NpmSource {
        package: "@acme/tool".into(),
        dist_tag: "latest".into(),
    })
}

fn fake_npm() -> FakeTools {
    FakeTools::new().tool(
        "npm",
        r#"
case "$1" in
  view) echo '{"versions":["1.0.0","1.1.0","1.2.0"],"dist-tags":{"latest":"1.2.0"}}';;
  ls)
    if [ -f "$HEADWAY_TEST_VERSION" ]; then
      version=$(cat "$HEADWAY_TEST_VERSION")
      printf '{"dependencies":{"@acme/tool":{"version":"%s"}}}\n' "$version"
    else echo '{}'; exit 1; fi;;
  install) printf '%s' "${3##*@}" > "$HEADWAY_TEST_VERSION";;
  *) exit 2;;
esac
"#,
    )
}

#[tokio::test]
async fn checks_discover_external_npm_updates_without_creating_history() {
    // Given an external installation and an inaccessible GitHub keychain.
    let state = tempfile::tempdir().unwrap();
    let version = state.path().join("version");
    std::fs::write(&version, "1.0.0").unwrap();
    let tools = fake_npm();
    let (_root, hw) = headway(
        tools
            .env()
            .with_var("HEADWAY_TEST_VERSION", version.display().to_string()),
    );
    hw.add(app(npm())).unwrap();
    hw.check("tool").await.unwrap();
    std::fs::write(version, "1.2.0").unwrap();

    // When npm was upgraded outside Headway and a check runs.
    let view = hw.check("tool").await.unwrap();

    // Then the system version wins without fabricating an install event.
    assert_eq!(view.installed.unwrap().id, "1.2.0");
    assert_eq!(view.status, AppStatus::UpToDate);
    assert!(hw.history("tool").unwrap().is_empty());
}

#[tokio::test]
async fn npm_updates_can_step_and_roll_back_using_the_detected_version() {
    // Given an npm-managed tool outside Headway's install history.
    let state = tempfile::tempdir().unwrap();
    let version = state.path().join("version");
    std::fs::write(&version, "1.0.0").unwrap();
    let tools = fake_npm();
    let (_root, hw) = headway(
        tools
            .env()
            .with_var("HEADWAY_TEST_VERSION", version.display().to_string()),
    );
    hw.add(app(npm())).unwrap();

    // When installing just the next published version.
    let outcome = hw
        .update("tool", &Target::Next, &NullSink, &CancellationToken::new())
        .await
        .unwrap();

    // Then the actual version, baseline and history agree.
    assert_eq!(outcome.from.unwrap().id, "1.0.0");
    assert_eq!(outcome.to.id, "1.1.0");
    assert_eq!(std::fs::read_to_string(version).unwrap(), "1.1.0");
    assert_eq!(hw.get("tool").unwrap().pending_count, 1);
    assert_eq!(hw.history("tool").unwrap()[0].to, "1.1.0");
}

#[tokio::test]
async fn npm_rollback_refreshes_available_updates() {
    // Given a tool whose installed version is the newest one.
    let state = tempfile::tempdir().unwrap();
    let version = state.path().join("version");
    std::fs::write(&version, "1.2.0").unwrap();
    let tools = fake_npm();
    let (_root, hw) = headway(
        tools
            .env()
            .with_var("HEADWAY_TEST_VERSION", version.display().to_string()),
    );
    hw.add(app(npm())).unwrap();

    // When explicitly rolling back.
    let outcome = hw
        .update(
            "tool",
            &Target::Ref("1.0.0".into()),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    // Then the installed version and pending updates reflect the rollback.
    assert_eq!(outcome.to.id, "1.0.0");
    assert_eq!(hw.get("tool").unwrap().pending_count, 2);
}

#[tokio::test]
async fn npm_latest_does_not_downgrade_a_newer_installed_version() {
    // Given an installed version ahead of the followed dist-tag.
    let state = tempfile::tempdir().unwrap();
    let version = state.path().join("version");
    std::fs::write(&version, "2.0.0").unwrap();
    let tools = fake_npm();
    let (_root, hw) = headway(
        tools
            .env()
            .with_var("HEADWAY_TEST_VERSION", version.display().to_string()),
    );
    hw.add(app(npm())).unwrap();
    assert!(hw.plan("tool", &Target::Latest).await.unwrap().up_to_date);
    // When requesting an ordinary latest update.
    let outcome = hw
        .update(
            "tool",
            &Target::Latest,
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    // Then an explicit version is required to downgrade it.
    assert!(!outcome.changed);
    assert_eq!(std::fs::read_to_string(version).unwrap(), "2.0.0");
}

#[tokio::test]
async fn failed_npm_queries_preserve_the_previous_installed_version() {
    // Given an existing installation and a broken npm query.
    let tools = fake_npm().tool("npm", r#"case "$1" in view) echo '{"versions":["1.2.0"],"dist-tags":{"latest":"1.2.0"}}';; ls) echo 'permission denied' >&2; exit 2;; esac"#);
    let (_root, hw) = headway(tools.env());
    hw.add(app(npm())).unwrap();
    hw.store
        .update_state(|state| {
            Ok((
                state.with_app("tool", |state| AppState {
                    installed: Some(InstalledRef {
                        id: "1.0.0".into(),
                        version: Some("1.0.0".into()),
                        sha: None,
                        path: None,
                        installed_at: Utc::now(),
                    }),
                    ..state
                }),
                (),
            ))
        })
        .unwrap();

    // When the query fails.
    let error = hw.check("tool").await.unwrap_err();

    // Then it is a check error, never an uninstall.
    assert_eq!(error.code(), "command_failed");
    assert_eq!(hw.get("tool").unwrap().installed.unwrap().id, "1.0.0");
}

#[tokio::test]
async fn updater_only_scripts_run_every_time_and_stay_unknown() {
    // Given an updater without any version probes.
    let state = tempfile::tempdir().unwrap();
    let runs = state.path().join("runs");
    let (_root, hw) =
        headway(ShellEnv::from_vars([]).with_var("HEADWAY_TEST_RUNS", runs.display().to_string()));
    hw.add(app(Source::Script(ScriptSource {
        update_command: "echo run >> \"$HEADWAY_TEST_RUNS\"".into(),
        version_command: None,
        latest_command: None,
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

    // When the user explicitly runs the updater again.
    hw.update(
        "tool",
        &Target::Latest,
        &NullSink,
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    // Then an unknown target is never used as evidence that it is current.
    assert_eq!(std::fs::read_to_string(runs).unwrap().lines().count(), 2);
    assert_eq!(hw.get("tool").unwrap().status, AppStatus::Unknown);
    assert!(!hw.plan("tool", &Target::Latest).await.unwrap().up_to_date);
}

#[tokio::test]
async fn scripts_record_the_observed_version_when_the_updater_changes_nothing() {
    // Given an updater reporting success without changing its old version.
    let (_root, hw) = headway(ShellEnv::from_vars([]));
    hw.add(app(Source::Script(ScriptSource {
        update_command: "true".into(),
        version_command: Some("echo 1.0.0".into()),
        latest_command: Some("echo 1.1.0".into()),
    })))
    .unwrap();

    // When the updater runs.
    let outcome = hw
        .update(
            "tool",
            &Target::Latest,
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    // Then Headway records what is actually installed and keeps the update pending.
    assert!(!outcome.changed);
    assert_eq!(outcome.to.id, "1.0.0");
    assert_eq!(hw.get("tool").unwrap().status, AppStatus::UpdateAvailable);
}

#[tokio::test]
async fn switching_source_forgets_installed_state_and_mark_is_rejected() {
    // Given a checked script and a different new package identity.
    let (_root, hw) = headway(ShellEnv::from_vars([]));
    hw.add(app(Source::Script(ScriptSource {
        update_command: "true".into(),
        version_command: Some("echo 1.0.0".into()),
        latest_command: None,
    })))
    .unwrap();
    hw.check("tool").await.unwrap();

    // When switching the app to npm.
    let view = hw.replace(app(npm())).unwrap();

    // Then old source state cannot be reused or manually marked for the new source.
    assert!(view.installed.is_none());
    assert!(!view.busy);
    assert!(view.last_check.is_none());
    assert_eq!(
        hw.mark("tool", "latest").await.unwrap_err().code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn brew_and_script_version_picks_are_rejected_before_installing() {
    // Given sources whose updaters only install their own latest version.
    let (_root, hw) = headway(ShellEnv::from_vars([]));
    let gh = GitHub::new("http://127.0.0.1:9", None).unwrap();
    for source in [
        Source::Brew(BrewSource {
            package: "tool".into(),
            cask: false,
        }),
        Source::Script(ScriptSource {
            update_command: "false".into(),
            version_command: None,
            latest_command: None,
        }),
    ] {
        // When requesting an arbitrary version.
        let error = hw
            .resolve(
                &gh,
                &app(source),
                None,
                &Target::Ref("1.0.0".into()),
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        // Then the invalid target is rejected without invoking an updater.
        assert_eq!(error.code(), "invalid_input");
    }
}
