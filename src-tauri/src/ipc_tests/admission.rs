//! Run admission: a refused start is reported, never silently accepted.

use serde_json::json;
use tauri::Manager as _;

use super::support::{call, harness, ok, tool_app};
use crate::runner::{self, Origin};
use crate::state::{AppState, ExitDisposition, prepare_exit};

fn app_with_dest(harness: &super::support::Harness) -> serde_json::Value {
    tool_app("file:///nowhere", &harness.home.path().join("bin/tool"))
}

#[test]
fn start_update_reports_busy_when_a_restart_closed_admission() {
    let harness = harness("http://127.0.0.1:9");
    let app = app_with_dest(&harness);
    ok(&harness, "add_app", json!({ "app": app }));
    let state = harness.app.state::<AppState>();
    // A self-update restart is admitted: new runs must be refused, not started.
    assert!(state.try_admit_restart_if_idle());
    let error = call(
        &harness,
        "start_update",
        json!({ "id": "tool", "target": "latest" }),
    )
    .unwrap_err();
    assert_eq!(error["code"], "busy");
    assert_eq!(
        state.active_runs(),
        0,
        "a refused start must not leave a tracked run (the window would spin forever)"
    );
    // Revisions use the same admission error.
    let reference = "a".repeat(40);
    let error = call(
        &harness,
        "install_revision",
        json!({ "id": "tool", "request": { "reference": reference, "method": "source" } }),
    )
    .unwrap_err();
    assert_eq!(error["code"], "busy");
    assert_eq!(state.active_runs(), 0);
    state.cancel_restart_admission();
    // Admission reopens after the restart attempt failed.
    assert!(state.begin_run("tool").is_some());
    state.end_run("tool");
}

#[tokio::test(flavor = "multi_thread")]
async fn background_run_update_reports_a_refused_admission() {
    let harness = harness("http://127.0.0.1:9");
    let app = app_with_dest(&harness);
    ok(&harness, "add_app", json!({ "app": app }));
    let state = harness.app.state::<AppState>();
    assert!(state.try_admit_restart_if_idle());
    let ran = runner::run_update(
        harness.app.handle(),
        "tool",
        &headway_core::Target::Latest,
        headway_core::RevisionMethod::Configured,
        Origin::User,
    )
    .await;
    assert!(!ran, "the caller must learn that nothing ran");
    assert_eq!(state.active_runs(), 0);
}

#[test]
fn quit_blocks_once_per_drain_and_never_exits_with_a_run_in_flight() {
    // Nothing running: the exit request proceeds.
    let idle = harness("http://127.0.0.1:9");
    assert_eq!(
        prepare_exit(&idle.app.state::<AppState>()),
        ExitDisposition::Exit
    );

    // A build is running: cancel it and block. The drain is claimed exactly once.
    let running = harness("http://127.0.0.1:9");
    let state = running.app.state::<AppState>();
    let run = state.begin_run("tool").unwrap();
    assert_eq!(
        prepare_exit(&state),
        ExitDisposition::Block { start_drain: true }
    );
    // A second quit request keeps waiting: it must not exit with the build alive.
    assert_eq!(
        prepare_exit(&state),
        ExitDisposition::Block { start_drain: false }
    );
    // Cancellation ends the run; the next request may exit.
    run.cancel();
    state.end_run("tool");
    assert_eq!(prepare_exit(&state), ExitDisposition::Exit);

    // A drain that ran out of grace lets the exit through even with runs left.
    let forced_harness = harness("http://127.0.0.1:9");
    let state = forced_harness.app.state::<AppState>();
    let _run = state.begin_run("other").unwrap();
    state.force_exit();
    assert_eq!(prepare_exit(&state), ExitDisposition::Exit);
}
