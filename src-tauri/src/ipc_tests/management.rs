use serde_json::json;
use tauri::Manager as _;

use super::packages::package_harness;
use super::support::{call, ok};
use crate::state::AppState;

#[test]
fn recipes_inventory_import_and_resume_use_the_registered_payloads() {
    // Given the actual IPC handler and isolated package-manager executables.
    let harness = package_harness();
    let recipes = ok(&harness, "list_recipes", json!({}));
    let recipe = recipes
        .as_array()
        .unwrap()
        .iter()
        .find(|recipe| recipe["id"] == "pnpm-npm")
        .unwrap();
    let discovered = ok(&harness, "discover_apps", json!({"sources":["npm"]}));
    assert_eq!(discovered["candidates"][0]["app"]["source"], "npm");
    // When importing the editable recipe through IPC.
    let imported = ok(&harness, "import_apps", json!({"apps":[recipe["app"]]}));
    // Then the app is tracked with unattended updating disabled.
    assert_eq!(imported[0]["app"]["source"], "npm");
    assert_eq!(imported[0]["app"]["auto_update"], false);
    harness
        .app
        .state::<AppState>()
        .hw
        .store()
        .update_state(|state| {
            Ok((
                state.with_app("pnpm-npm", |state| headway_core::state::AppState {
                    auto_update_paused: true,
                    ..state
                }),
                (),
            ))
        })
        .unwrap();
    assert_eq!(
        ok(&harness, "resume_auto_update", json!({"id":"pnpm-npm"}))["auto_update_paused"],
        false
    );
}

#[test]
fn unsupported_restore_releases_run_admission_and_metadata_is_github_only() {
    // Given a tracked npm source and no active runs.
    let harness = package_harness();
    ok(
        &harness,
        "add_app",
        json!({"app":{"id":"npm","name":"npm","source":"npm","package":"@test/tool"}}),
    );
    // When a frontend requests a cached restore unavailable to this provider.
    let error = call(
        &harness,
        "restore_backup",
        json!({"id":"npm","backupId":"snapshot-1"}),
    )
    .unwrap_err();
    // Then the stable error is returned and no busy run leaks.
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(harness.app.state::<AppState>().active_runs(), 0);
    assert_eq!(
        call(
            &harness,
            "commit_metadata",
            json!({"id":"npm","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})
        )
        .unwrap_err()["code"],
        "invalid_input"
    );
}

#[test]
fn importing_a_cask_preserves_the_reported_location_in_the_ipc_contract() {
    // Given the registered handler and a cask found in a custom application directory.
    let harness = package_harness();
    let path = "/Applications/Custom Apps/Editor.app";
    // When the inventory selection is explicitly imported.
    let imported = ok(
        &harness,
        "import_apps",
        json!({"apps":[{"id":"editor","name":"Editor","kind":"app","source":"brew","package":"editor","cask":true,"auto_update":true,"install_path":path}]}),
    );
    // Then both cask ownership and its metadata are retained without inventing an installed ref.
    assert_eq!(imported[0]["app"]["install_path"], path);
    assert_eq!(imported[0]["app"]["cask"], true);
    assert_eq!(imported[0]["app"]["auto_update"], false);
    assert!(imported[0]["installed"].is_null());
    assert_eq!(
        ok(&harness, "get_app", json!({"id":"editor"}))["app"]["install_path"],
        path
    );
}
