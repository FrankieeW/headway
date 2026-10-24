//! Restrictions and cancellation use the same IPC/event surface as the window.

use std::time::Duration;

use serde_json::{Value, json};
use tauri::{Listener as _, Manager as _};
use tokio::sync::mpsc;

use super::packages::{executable, package_harness, script_app};
use super::support::{call, ok};
use crate::state::{AppState, UPDATE_EVENT, UPDATE_FINISHED};

#[test]
fn package_and_script_checks_report_installed_versions_and_unknown_status() {
    // Given package managers with installed versions, and a script without probes.
    let harness = package_harness();
    for (input, id, version) in [
        ("npm:@test/tool", "npm", "1.0.0"),
        ("brew:fixture", "formula", "2.0.0"),
        ("cask:fixture", "cask", "3.0.0"),
    ] {
        let mut app = ok(&harness, "lookup_source", json!({"input":input}))["suggestion"].clone();
        app["id"] = json!(id);
        ok(&harness, "add_app", json!({"app":app}));
        // When checking through IPC.
        let checked = ok(&harness, "check_app", json!({"id":id}));
        // Then the installed version and update flags come from core.
        assert_eq!(checked["installed"]["id"], version);
        assert_eq!(checked["status"], "update_available");
        assert!(checked["pending_count"].as_u64().unwrap() > 0);
    }
    ok(&harness, "add_app", json!({"app":script_app("script")}));
    let checked = ok(&harness, "check_app", json!({"id":"script"}));
    assert_eq!(checked["status"], "unknown");
    assert_eq!(checked["last_check"]["status"], "unknown");
    assert_eq!(checked["pending_count"], 0);
    let summary = ok(&harness, "check_all", json!({}));
    assert_eq!(summary, json!({"checked":4, "with_updates":3, "failed":[]}));
}

#[test]
fn core_restrictions_surface_as_ipc_errors() {
    // Given non-GitHub apps whose versions are detected from the system.
    let harness = package_harness();
    let apps = [
        json!({"id":"brew", "name":"brew", "source":"brew", "package":"fixture"}),
        json!({"id":"npm", "name":"npm", "source":"npm", "package":"@test/tool"}),
        script_app("script"),
    ];
    for app in apps {
        let id = app["id"].as_str().unwrap();
        ok(&harness, "add_app", json!({"app":app}));
        // When a frontend tries to set a manual baseline.
        let error = call(
            &harness,
            "mark_installed",
            json!({"id":id, "reference":"1.0.0"}),
        )
        .unwrap_err();
        // Then core's stable restriction error reaches the window.
        assert_eq!(error["code"], "invalid_input");
        if id != "npm" {
            for target in [json!("next"), json!({"ref":"1.0.0"})] {
                let error =
                    call(&harness, "plan_update", json!({"id":id, "target":target})).unwrap_err();
                assert_eq!(error["code"], "invalid_input");
            }
        }
    }
    assert!(!harness.home.path().join("updated").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn unsupported_update_target_reports_failure_without_running_script() {
    // Given a script and a listener attached before starting it.
    let harness = package_harness();
    ok(&harness, "add_app", json!({"app":script_app("script")}));
    let (send, mut receive) = mpsc::channel(1);
    harness.app.listen(UPDATE_FINISHED, move |event| {
        send.try_send(serde_json::from_str::<Value>(event.payload()).unwrap())
            .unwrap();
    });
    // When the frontend asks to install a specific script version.
    ok(
        &harness,
        "start_update",
        json!({"id":"script", "target":{"ref":"1.0.0"}}),
    );
    let finished = tokio::time::timeout(Duration::from_secs(10), receive.recv())
        .await
        .unwrap()
        .unwrap();
    // Then the runner reports the core rejection and never invokes the updater.
    assert_eq!(finished["app_id"], "script");
    assert_eq!(finished["ok"], false);
    assert_eq!(finished["error"]["code"], "invalid_input");
    assert!(!harness.home.path().join("updated").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn running_script_can_be_cancelled_through_ipc() {
    // Given an updater that announces readiness and then blocks until cancelled.
    let harness = package_harness();
    executable(
        &harness.home.path().join("tools/fixture-update"),
        "printf 'READY\\n'\nexec /bin/sleep 60",
    );
    let mut app = script_app("script");
    app["update_command"] = json!("fixture-update");
    ok(&harness, "add_app", json!({"app":app}));
    let (ready_send, mut ready) = mpsc::channel(1);
    harness.app.listen(UPDATE_EVENT, move |event| {
        let event: Value = serde_json::from_str(event.payload()).unwrap();
        if event["type"] == "log" && event["stream"] == "stdout" && event["line"] == "READY" {
            ready_send.try_send(()).unwrap();
        }
    });
    let (send, mut receive) = mpsc::channel(1);
    harness.app.listen(UPDATE_FINISHED, move |event| {
        send.try_send(serde_json::from_str::<Value>(event.payload()).unwrap())
            .unwrap();
    });
    ok(
        &harness,
        "start_update",
        json!({"id":"script", "target":"latest"}),
    );
    tokio::time::timeout(Duration::from_secs(10), ready.recv())
        .await
        .unwrap()
        .unwrap();
    // When the window cancels a confirmed running child process.
    assert_eq!(ok(&harness, "cancel_update", json!({"id":"script"})), true);
    let finished = tokio::time::timeout(Duration::from_secs(10), receive.recv())
        .await
        .unwrap()
        .unwrap();
    // Then cancellation is reported through the completion event and run tracking is released.
    assert_eq!(finished["app_id"], "script");
    assert_eq!(finished["ok"], false);
    assert_eq!(finished["error"]["code"], "cancelled");
    // The event precedes the run guard's drop; await the task's cleanup without a sleep.
    tokio::time::timeout(Duration::from_secs(10), async {
        while harness.app.state::<AppState>().active_runs() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(ok(&harness, "cancel_update", json!({"id":"script"})), false);
    assert_eq!(
        ok(&harness, "get_app", json!({"id":"script"}))["busy"],
        false
    );
}
