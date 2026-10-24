//! Real handler contract checks; all bundles and links live in a temporary home.
use crate::ipc_tests::support::{Harness, call, harness, ok};
use crate::state::AppState;
use serde_json::json;
use std::os::unix::fs::PermissionsExt as _;
use tauri::Manager as _;

fn fixture() -> Harness {
    let h = harness("http://127.0.0.1:9");
    let root = h.home.path().join("apps");
    let app = root.join("Headway.app/Contents");
    std::fs::create_dir_all(app.join("MacOS")).unwrap();
    std::fs::write(app.join("Info.plist"), r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>wang.frankie.headway</string><key>CFBundleExecutable</key><string>headway-desktop</string></dict></plist>"#).unwrap();
    for name in ["headway", "headway-desktop"] {
        let path = app.join("MacOS").join(name);
        std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"data\":{\"version\":\"9.0.0\",\"commit\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"dirty\":false,\"target\":null}}'\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let state = h.app.state::<AppState>();
    state
        .hw
        .set_setting("apps_dir", root.to_str().unwrap())
        .unwrap();
    state
        .hw
        .set_setting("bin_dir", h.home.path().join("bin").to_str().unwrap())
        .unwrap();
    h
}

#[test]
fn ipc_cli_repair_returns_extended_status() {
    let h = fixture();
    let before = ok(&h, "cli_status", json!({}));
    assert_eq!(before["needs_repair"], true);
    let after = ok(&h, "fix_cli_link", json!({"appPath": null}));
    assert_eq!(after["installed"], true);
    assert_eq!(after["needs_repair"], false);
    assert!(after["shadowed"].is_boolean());
    assert!(
        std::fs::read_link(h.home.path().join("bin/headway"))
            .unwrap()
            .ends_with("Headway.app/Contents/MacOS/headway")
    );
}

#[test]
fn ipc_restart_request_is_pending_and_not_a_confirmed_launch() {
    let h = fixture();
    let before = ok(&h, "self_update_status", json!({}));
    assert_eq!(before["installed"]["version"], "9.0.0");
    assert_eq!(before["restart_requested"], false);
    let requested = ok(&h, "request_self_restart", json!({}));
    assert_eq!(requested["restart_requested"], true);
    assert_eq!(requested["confirmed"], false);
    let state = h.app.state::<AppState>();
    assert!(
        state
            .hw
            .paths()
            .config_dir
            .join("self-restart.json")
            .exists()
    );
}

#[test]
fn ipc_standalone_restart_errors_without_desktop() {
    let h = fixture();
    h.app
        .state::<AppState>()
        .hw
        .set_setting("apps_dir", h.home.path().join("absent").to_str().unwrap())
        .unwrap();
    let error = call(&h, "request_self_restart", json!({})).unwrap_err();
    assert_eq!(error["code"], "invalid_input");
}

#[test]
fn restart_gate_defers_busy_and_blocks_new_runs_after_admission() {
    let h = fixture();
    let state = h.app.state::<AppState>();
    assert!(state.begin_run("test").is_some());
    assert!(!state.try_admit_restart_if_idle());
    state.end_run("test");
    assert!(state.try_admit_restart_if_idle());
    assert!(state.begin_run("later").is_none());
    state.cancel_restart_admission();
    assert!(state.begin_run("later").is_some());
}

#[test]
fn restart_idle_check_defers_external_cli_lock() {
    let h = fixture();
    let dest = h.home.path().join("bin/tool");
    ok(
        &h,
        "add_app",
        json!({"app": crate::ipc_tests::support::tool_app("file:///nowhere", &dest)}),
    );
    let state = h.app.state::<AppState>();
    let held =
        headway_core::lock::AppLock::acquire(&state.hw.paths().app_lock("tool"), "tool").unwrap();
    assert_eq!(state.active_runs(), 0);
    assert!(!super::updates_idle(&state).unwrap());
    drop(held);
    assert!(super::updates_idle(&state).unwrap());
}

#[test]
fn ipc_failed_confirmation_exposes_error_and_allows_retry() {
    let h = fixture();
    ok(&h, "request_self_restart", json!({}));
    let state = h.app.state::<AppState>();
    let path = state.hw.paths().config_dir.join("self-restart.json");
    let mut request: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    request["dispatched"] = json!(true);
    // Target this simulated startup, rather than another installation.
    request["app_path"] = json!(
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .display()
            .to_string()
    );
    std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    assert!(!state.hw.confirm_self_restart().unwrap());
    let failed = ok(&h, "self_update_status", json!({}));
    assert_eq!(failed["restart_requested"], false);
    assert_eq!(failed["confirmed"], false);
    assert!(failed["restart_error"].is_string());
    let retry = ok(&h, "request_self_restart", json!({}));
    assert_eq!(retry["restart_requested"], true);
    assert!(retry["restart_error"].is_null());
}
