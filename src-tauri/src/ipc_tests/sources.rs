//! Source payloads and lookup results through the registered command handler.

use serde_json::{Value, json};

use super::packages::{package_harness, script_app};
use super::support::{call, ok, tool_app};

#[test]
fn source_apps_round_trip_through_add_save_and_list() {
    // Given every non-GitHub source, including a cask and optional script probes.
    let harness = package_harness();
    let apps = [
        json!({"id":"npm", "name":"npm", "source":"npm", "package":"@test/tool", "dist_tag":"latest", "kind":"cli", "auto_update":false}),
        json!({"id":"brew", "name":"brew", "source":"brew", "package":"fixture", "cask":false, "kind":"cli", "auto_update":false}),
        json!({"id":"cask", "name":"cask", "source":"brew", "package":"fixture", "cask":true, "kind":"app", "auto_update":false}),
        script_app("script"),
    ];
    // When the frontend adds and edits each source.
    for mut app in apps {
        let added = ok(&harness, "add_app", json!({"app":app}));
        app["update_policy"] = json!({"mode":"latest","max_steps":1,"pause_on_failure":true});
        assert_eq!(added["app"], app);
        app["name"] = json!(format!("Edited {}", app["id"].as_str().unwrap()));
        app["auto_update"] = json!(true);
        match app["source"].as_str().unwrap() {
            "npm" => app["dist_tag"] = json!("beta"),
            "brew" => app["package"] = json!("other"),
            "script" => {
                app["update_command"] = json!("printf changed");
                app["version_command"] = json!("printf 1.0.0");
                app["latest_command"] = json!("printf 2.0.0");
            }
            source => panic!("unexpected fixture source {source}"),
        }
        let saved = ok(&harness, "save_app", json!({"app":app}));
        // Then both the command result and persisted list retain source-specific fields.
        assert_eq!(saved["app"], app);
        let listed = ok(&harness, "list_apps", json!({}));
        let listed = listed
            .as_array()
            .unwrap()
            .iter()
            .find(|view| view["app"]["id"] == app["id"])
            .unwrap();
        assert_eq!(listed["app"], app);
    }
}

#[test]
fn lookup_source_returns_package_metadata_and_addable_suggestions() {
    // Given deterministic executables instead of the user's package managers.
    let harness = package_harness();
    for (input, source, latest, installed, description, page, kind) in [
        (
            "npm:@test/tool",
            "npm",
            "2.0.0",
            "1.0.0",
            "Fixture npm",
            "npm",
            "cli",
        ),
        (
            "brew:fixture",
            "brew",
            "3.0.0",
            "2.0.0",
            "Fixture formula",
            "formula",
            "cli",
        ),
        (
            "cask:fixture",
            "brew",
            "4.0.0",
            "3.0.0",
            "Fixture cask",
            "cask",
            "app",
        ),
    ] {
        // When the frontend looks up a source.
        let lookup = ok(&harness, "lookup_source", json!({"input":input}));
        // Then all public fields carry the package manager's actual response.
        assert_eq!(lookup["source"], source);
        assert_eq!(lookup["package"], input.split_once(':').unwrap().1);
        assert_eq!(lookup["description"], description);
        assert_eq!(
            lookup["homepage"],
            format!("https://example.invalid/{page}")
        );
        assert_eq!(lookup["latest"], latest);
        assert_eq!(lookup["installed"], installed);
        assert!(!lookup["notes"].as_array().unwrap().is_empty());
        let mut suggestion = lookup["suggestion"].clone();
        assert_eq!(suggestion["source"], source);
        assert_eq!(suggestion["kind"], kind);
        assert_eq!(suggestion["auto_update"], false);
        if source == "brew" {
            assert_eq!(suggestion["cask"], page == "cask");
        } else {
            assert_eq!(suggestion["dist_tag"], "latest");
        }
        suggestion["id"] = json!(page);
        let added = ok(&harness, "add_app", json!({"app":suggestion}));
        assert_eq!(added["app"], suggestion);
    }
}

#[test]
fn lookup_source_rejects_unsupported_and_invalid_inputs() {
    // Given a registered lookup handler.
    let harness = package_harness();
    for input in ["script:tool", "octo/tool", "npm:Bad Name", "brew:a/b"] {
        // When lookup cannot resolve this source type or package syntax.
        let error = call(&harness, "lookup_source", json!({"input":input})).unwrap_err();
        // Then the stable command error contract is preserved.
        assert_eq!(error["code"], "invalid_input");
        assert!(error["message"].is_string());
    }
}

#[test]
fn legacy_github_payload_and_config_remain_readable() {
    // Given the frontend's old source-less GitHub payload.
    let harness = package_harness();
    let dest = harness.home.path().join("bin/tool");
    let mut legacy = tool_app("file:///nowhere", &dest);
    legacy.as_object_mut().unwrap().remove("source");
    let added = ok(&harness, "add_app", json!({"app":legacy}));
    assert_eq!(added["app"]["source"], "github");
    legacy["name"] = json!("Legacy edited");
    let saved = ok(&harness, "save_app", json!({"app":legacy}));
    assert_eq!(saved["app"]["name"], "Legacy edited");
    // When the config on disk also lacks the new discriminator.
    let config = harness.home.path().join("config/config.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(text.contains("source = \"github\""));
    std::fs::write(&config, text.replace("source = \"github\"\n", "")).unwrap();
    let listed = ok(&harness, "list_apps", json!({}));
    // Then the old config is normalized without losing the repository or install settings.
    assert_eq!(listed[0]["app"], saved["app"]);
    assert_eq!(listed[0]["app"]["repo"], "octo/tool");
    assert_ne!(listed[0]["app"]["install"], Value::Null);
}

#[test]
fn inspect_repo_rejects_input_that_is_not_a_repository() {
    let harness = super::support::harness("http://127.0.0.1:9");
    for input in ["", "not a repo", "https://github.com/only-owner", "owner/"] {
        let error = call(&harness, "inspect_repo", json!({"repo": input})).unwrap_err();
        assert_eq!(error["code"], "invalid_input", "accepted `{input}`");
    }
}
