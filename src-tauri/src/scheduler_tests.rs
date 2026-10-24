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
