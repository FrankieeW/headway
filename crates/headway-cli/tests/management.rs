//! Machine contracts through the real CLI, without changing the user's installations.
#![allow(clippy::unwrap_used)]
use assert_cmd::Command;
use serde_json::{Value, json};

fn command(home: &std::path::Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("headway"));
    command
        .env("HEADWAY_HOME", home)
        .env("HEADWAY_API_URL", "http://127.0.0.1:1")
        .env("HEADWAY_GITHUB_TOKEN", "test-token")
        .env("NO_COLOR", "1")
        .env_remove("HEADWAY_OUTPUT");
    command
}

fn document(home: &std::path::Path, args: &[&str]) -> Value {
    let output = command(home)
        .arg("--json")
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn recipes_can_be_previewed_and_imported_without_installing() {
    // Given an isolated home with no subscriptions.
    let root = tempfile::tempdir().unwrap();
    let catalog = document(root.path(), &["recipes"]);
    assert!(
        catalog["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == "pnpm-npm")
    );
    let preview = document(root.path(), &["import", "recipe:pnpm-npm", "--dry-run"]);
    assert_eq!(preview["data"]["saved"], false);
    assert!(!root.path().join("config/config.toml").exists());
    // When the chosen recipe is imported.
    let imported = document(root.path(), &["import", "recipe:pnpm-npm"]);
    // Then only tracking is saved, without opting into unattended updates.
    assert_eq!(imported["data"]["saved"], true);
    assert_eq!(imported["data"]["apps"][0]["app"]["source"], "npm");
    assert_eq!(imported["data"]["apps"][0]["app"]["auto_update"], false);
}

#[test]
fn duplicate_import_is_rejected_even_during_dry_run() {
    // Given an already tracked recipe.
    let root = tempfile::tempdir().unwrap();
    document(root.path(), &["import", "recipe:pnpm-npm"]);
    let before = std::fs::read(root.path().join("config/config.toml")).unwrap();
    // When the same recipe is planned again.
    let output = command(root.path())
        .args(["--json", "import", "recipe:pnpm-npm", "--dry-run"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    // Then the plan is an error and the existing configuration remains exact.
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(
        std::fs::read(root.path().join("config/config.toml")).unwrap(),
        before
    );
}

#[test]
fn recipe_ai_output_is_one_xml_document() {
    let root = tempfile::tempdir().unwrap();
    let output = command(root.path())
        .args(["--ai", "recipes"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    let xml = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(xml.root_element().attribute("command"), Some("recipes"));
}

#[test]
fn policy_resume_preserves_the_saved_app_and_commit_metadata_rejects_npm() {
    // Given a paused npm subscription in an isolated store.
    let root = tempfile::tempdir().unwrap();
    document(root.path(), &["import", "recipe:pnpm-npm"]);
    let hw = headway_core::Headway::builder()
        .paths(headway_core::paths::Paths::under(root.path()))
        .build()
        .unwrap();
    hw.store()
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
    // When the CLI resumes policy execution.
    let resumed = document(root.path(), &["resume-auto-update", "pnpm-npm"]);
    // Then the runtime pause clears while opt-in remains unchanged.
    assert_eq!(resumed["data"]["auto_update_paused"], false);
    assert_eq!(resumed["data"]["app"]["auto_update"], false);
    let output = command(root.path())
        .args([
            "--json",
            "commit-info",
            "pnpm-npm",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let rejected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rejected["error"]["code"], json!("invalid_input"));
}
