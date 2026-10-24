//! Drive a complete scheduler cycle with isolated scripts and real core checks.

use headway_core::lock::AppLock;
use serde_json::json;
use tauri::Manager as _;

use crate::ipc_tests::packages::{executable, package_harness, script_app};
use crate::ipc_tests::support::ok;
use crate::state::AppState;

#[tokio::test(flavor = "multi_thread")]
async fn cycle_only_runs_known_updates_that_opt_in_and_are_idle() {
    // Given unknown, opted-out, busy, current, and eligible script sources.
    let harness = package_harness();
    executable(
        &harness.home.path().join("tools/fixture-update"),
        "printf updated > \"$FIXTURE_HOME/$1.ran\"",
    );
    for (id, auto, version, latest) in [
        ("unknown", true, None, None),
        ("unknown-installed", true, Some("1.0.0"), None),
        ("unknown-latest", true, None, Some("2.0.0")),
        ("opted-out", false, Some("1.0.0"), Some("2.0.0")),
        ("busy", true, Some("1.0.0"), Some("2.0.0")),
        ("current", true, Some("2.0.0"), Some("2.0.0")),
        ("eligible", true, Some("1.0.0"), Some("2.0.0")),
    ] {
        let mut app = script_app(id);
        app["auto_update"] = json!(auto);
        app["update_command"] = json!(format!("fixture-update {id}"));
        app["version_command"] = json!(version.map(|v| format!("printf {v}")));
        app["latest_command"] = json!(latest.map(|v| format!("printf {v}")));
        ok(&harness, "add_app", json!({"app":app}));
    }
    let mut settings = ok(&harness, "get_settings", json!({}));
    settings["notify"] = json!(false);
    ok(&harness, "save_settings", json!({"settings":settings}));
    let state = harness.app.state::<AppState>();
    let _busy = AppLock::acquire(&state.hw.paths().app_lock("busy"), "busy").unwrap();
    // When one full background check/update cycle completes.
    super::run_cycle(harness.app.handle()).await;
    // Then only the known, opted-in, idle update executed.
    assert_eq!(
        std::fs::read_to_string(harness.home.path().join("eligible.ran")).unwrap(),
        "updated"
    );
    for id in [
        "unknown",
        "unknown-installed",
        "unknown-latest",
        "opted-out",
        "busy",
        "current",
    ] {
        assert!(
            !harness.home.path().join(format!("{id}.ran")).exists(),
            "unexpected update: {id}"
        );
    }
    for id in ["unknown", "unknown-installed", "unknown-latest"] {
        let view = ok(&harness, "get_app", json!({"id":id}));
        assert_eq!(view["status"], "unknown");
        assert_eq!(view["pending_count"], 0);
    }
    assert_eq!(
        ok(&harness, "get_app", json!({"id":"opted-out"}))["status"],
        "update_available"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cycle_respects_next_latest_limits_and_notify() {
    for (mode, steps, expected) in [
        ("next", 1, "1.1.0\n"),
        ("next", 2, "1.1.0\n1.2.0\n"),
        ("latest", 2, "1.2.0\n"),
        ("notify", 2, ""),
    ] {
        // Given a real fixture npm with two versions pending.
        let harness = package_harness();
        std::fs::write(harness.home.path().join("version"), "1.0.0").unwrap();
        executable(
            &harness.home.path().join("tools/npm"),
            r#"
case "$1" in
 view) echo '{"versions":["1.0.0","1.1.0","1.2.0"],"dist-tags":{"latest":"1.2.0"}}';;
 ls) printf '{"dependencies":{"@test/tool":{"version":"%s"}}}' "$(cat "$FIXTURE_HOME/version")";;
 install)
   printf '%s' "${3##*@}" > "$FIXTURE_HOME/version"
   printf '%s\n' "${3##*@}" >> "$FIXTURE_HOME/runs";;
 *) exit 64;;
esac
"#,
        );
        ok(
            &harness,
            "add_app",
            json!({"app": {
                "id":"tool", "name":"Tool", "source":"npm", "package":"@test/tool",
                "auto_update":true, "update_policy":{"mode":mode,"max_steps":steps}
            }}),
        );
        let mut settings = ok(&harness, "get_settings", json!({}));
        settings["notify"] = json!(false);
        ok(&harness, "save_settings", json!({"settings":settings}));
        // When the complete scheduler cycle executes.
        super::run_cycle(harness.app.handle()).await;
        // Then the subprocess trace matches the configured policy exactly.
        let runs = harness.home.path().join("runs");
        if expected.is_empty() {
            assert!(!runs.exists());
        } else {
            assert_eq!(std::fs::read_to_string(runs).unwrap(), expected);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cycle_pauses_failure_and_resume_allows_retry() {
    // Given a known update whose script fails.
    let harness = package_harness();
    let mut app = script_app("failing");
    app["auto_update"] = json!(true);
    app["version_command"] = json!("printf 1.0.0");
    app["latest_command"] = json!("printf 2.0.0");
    app["update_command"] = json!("echo run >> \"$FIXTURE_HOME/runs\"; exit 42");
    ok(&harness, "add_app", json!({"app":app}));
    let mut settings = ok(&harness, "get_settings", json!({}));
    settings["notify"] = json!(false);
    ok(&harness, "save_settings", json!({"settings":settings}));
    // When repeated cycles encounter the failure.
    super::run_cycle(harness.app.handle()).await;
    super::run_cycle(harness.app.handle()).await;
    // Then only one attempt ran, until explicit resume permits a retry.
    let state = harness.app.state::<AppState>();
    assert!(state.hw.get("failing").unwrap().auto_update_paused);
    assert_eq!(
        std::fs::read_to_string(harness.home.path().join("runs")).unwrap(),
        "run\n"
    );
    state.hw.resume_auto_update("failing").unwrap();
    super::run_cycle(harness.app.handle()).await;
    assert_eq!(
        std::fs::read_to_string(harness.home.path().join("runs")).unwrap(),
        "run\nrun\n"
    );
}
