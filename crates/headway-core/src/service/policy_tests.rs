use super::*;
use crate::lock::AppLock;
use crate::paths::Paths;
use crate::testing::FakeTools;
use crate::{AutoUpdateMode, NullSink, UpdatePolicy};
use crate::{Source, TrackedApp};

fn fixture(mode: AutoUpdateMode, steps: u8) -> (tempfile::TempDir, FakeTools, Headway) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("version"), "1.0.0").unwrap();
    let tools = FakeTools::new().tool(
        "npm",
        r#"
case "$1" in
 view) echo '{"versions":["1.0.0","1.1.0","1.2.0","1.3.0"],"dist-tags":{"latest":"1.3.0"}}';;
 ls) printf '{"dependencies":{"tool":{"version":"%s"}}}' "$(cat "$FIXTURE_ROOT/version")";;
 install)
   if [ -f "$FIXTURE_ROOT/fail" ]; then
     failure=$(cat "$FIXTURE_ROOT/fail")
     [ "$failure" != fail ] && [ "$failure" != "${3##*@}" ] || exit 42
   fi
   printf '%s' "${3##*@}" > "$FIXTURE_ROOT/version"
   printf '%s\n' "${3##*@}" >> "$FIXTURE_ROOT/installed";;
 *) exit 64;;
esac
"#,
    );
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .env(
            tools
                .env()
                .with_var("FIXTURE_ROOT", root.path().display().to_string()),
        )
        .build()
        .unwrap();
    hw.add(TrackedApp {
        id: "tool".into(),
        name: "Tool".into(),
        kind: crate::AppKind::Cli,
        auto_update: true,
        update_policy: UpdatePolicy {
            mode,
            max_steps: steps,
            ..UpdatePolicy::default()
        },
        source: Source::Npm(crate::NpmSource {
            package: "tool".into(),
            dist_tag: "latest".into(),
        }),
    })
    .unwrap();
    (root, tools, hw)
}

#[tokio::test]
async fn policy_next_is_bounded_and_latest_skips_intermediate_versions() {
    for (mode, steps, expected) in [
        (AutoUpdateMode::Next, 1, "1.1.0\n"),
        (AutoUpdateMode::Next, 2, "1.1.0\n1.2.0\n"),
        (AutoUpdateMode::Next, 20, "1.1.0\n1.2.0\n1.3.0\n"),
        (AutoUpdateMode::Latest, 20, "1.3.0\n"),
    ] {
        // Given an installed npm tool with three pending versions.
        let (root, _tools, hw) = fixture(mode, steps);
        // When one automatic cycle runs.
        let runs = hw
            .auto_update("tool", &NullSink, &CancellationToken::new())
            .await
            .unwrap();
        // Then real tool invocations follow the configured bound and target.
        assert_eq!(
            std::fs::read_to_string(root.path().join("installed")).unwrap(),
            expected
        );
        assert_eq!(runs.len(), expected.lines().count());
        assert_eq!(hw.history("tool").unwrap().len(), runs.len());
    }
}

#[tokio::test]
async fn policy_failure_pauses_until_explicit_resume() {
    // Given an updater that fails its first installation.
    let (root, _tools, hw) = fixture(AutoUpdateMode::Next, 2);
    std::fs::write(root.path().join("fail"), "fail").unwrap();
    // When the automatic attempt fails.
    let result = hw
        .auto_update("tool", &NullSink, &CancellationToken::new())
        .await;
    // Then it remains an error and future attempts are paused, including after manual success.
    assert!(result.is_err());
    assert!(hw.get("tool").unwrap().auto_update_paused);
    std::fs::remove_file(root.path().join("fail")).unwrap();
    assert!(
        hw.auto_update("tool", &NullSink, &CancellationToken::new())
            .await
            .unwrap()
            .is_empty()
    );
    hw.update("tool", &Target::Next, &NullSink, &CancellationToken::new())
        .await
        .unwrap();
    assert!(hw.get("tool").unwrap().auto_update_paused);
    assert!(!hw.resume_auto_update("tool").unwrap().auto_update_paused);
    let runs = hw
        .auto_update("tool", &NullSink, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(
        std::fs::read_to_string(root.path().join("version")).unwrap(),
        "1.3.0"
    );
}

#[tokio::test]
async fn policy_notify_disabled_and_busy_do_not_install() {
    for reason in ["notify", "disabled", "busy"] {
        // Given an ineligible automatic update.
        let (root, _tools, hw) = fixture(AutoUpdateMode::Notify, 1);
        let mut app = hw.get("tool").unwrap().app;
        if reason != "notify" {
            app.update_policy.mode = AutoUpdateMode::Latest;
        }
        if reason == "disabled" {
            app.auto_update = false;
        }
        hw.replace(app).unwrap();
        let _lock = (reason == "busy")
            .then(|| AppLock::acquire(&hw.paths().app_lock("tool"), "tool").unwrap());
        // When the policy is executed.
        let runs = hw
            .auto_update("tool", &NullSink, &CancellationToken::new())
            .await
            .unwrap();
        // Then no installation is attempted.
        assert!(runs.is_empty());
        assert!(!root.path().join("installed").exists());
    }
}

#[tokio::test]
async fn saved_notify_or_lower_step_budget_stops_the_remaining_automatic_cycle() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for change in ["notify", "lower_budget"] {
        // Given a three-step cycle whose first success changes the saved policy.
        let (root, _tools, hw) = fixture(AutoUpdateMode::Next, 3);
        let changed = AtomicBool::new(false);
        let sink = |event| {
            if matches!(
                event,
                crate::UpdateEvent::Stage {
                    stage: crate::Stage::Done,
                    ..
                }
            ) && !changed.swap(true, Ordering::SeqCst)
            {
                let mut app = hw.get("tool").unwrap().app;
                if change == "notify" {
                    app.update_policy.mode = AutoUpdateMode::Notify;
                } else {
                    app.update_policy.max_steps = 1;
                }
                hw.replace(app).unwrap();
            }
        };
        // When the automatic cycle runs and reaches that saved change.
        let runs = hw
            .auto_update("tool", &sink, &CancellationToken::new())
            .await
            .unwrap();
        // Then it never starts a second installation using the old policy.
        assert_eq!(runs.len(), 1);
        assert_eq!(
            std::fs::read_to_string(root.path().join("installed")).unwrap(),
            "1.1.0\n"
        );
        assert!(hw.get("tool").unwrap().app.auto_update);
    }
}

#[tokio::test]
async fn policy_pause_can_be_disabled() {
    // Given failure with pause disabled.
    let (root, _tools, hw) = fixture(AutoUpdateMode::Latest, 1);
    let mut app = hw.get("tool").unwrap().app;
    app.update_policy.pause_on_failure = false;
    hw.replace(app).unwrap();
    std::fs::write(root.path().join("fail"), "fail").unwrap();
    // When installation fails.
    let result = hw
        .auto_update("tool", &NullSink, &CancellationToken::new())
        .await;
    // Then the error propagates without pausing.
    assert!(result.is_err());
    assert!(!hw.get("tool").unwrap().auto_update_paused);
}

#[tokio::test]
async fn policy_cancelled_attempt_does_not_pause() {
    // Given an already-cancelled cycle.
    let (root, _tools, hw) = fixture(AutoUpdateMode::Next, 2);
    let cancel = CancellationToken::new();
    cancel.cancel();
    // When applying its policy.
    let result = hw.auto_update("tool", &NullSink, &cancel).await;
    // Then cancellation remains an error without recording a failed install.
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!hw.get("tool").unwrap().auto_update_paused);
    assert!(!root.path().join("installed").exists());
}

#[tokio::test]
async fn policy_failure_after_one_step_returns_error_and_stops() {
    // Given a second step that fails after the first succeeds.
    let (root, _tools, hw) = fixture(AutoUpdateMode::Next, 3);
    std::fs::write(root.path().join("fail"), "1.2.0").unwrap();
    // When a multi-step cycle runs.
    let result = hw
        .auto_update("tool", &NullSink, &CancellationToken::new())
        .await;
    // Then the cycle fails, retaining only the real first success and failed second attempt.
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(root.path().join("installed")).unwrap(),
        "1.1.0\n"
    );
    assert!(hw.get("tool").unwrap().auto_update_paused);
    let history = hw.history("tool").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history.iter().filter(|entry| entry.ok).count(), 1);
}
