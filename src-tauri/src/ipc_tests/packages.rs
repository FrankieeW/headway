//! Package executables confined to each test's injected PATH.

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use headway_core::shell::ShellEnv;
use serde_json::{Value, json};

use super::support::{Harness, harness_with_env};

pub(crate) fn executable(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub(crate) fn package_harness() -> Harness {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("tools");
    std::fs::create_dir(&bin).unwrap();
    executable(
        &bin.join("npm"),
        r#"
case "$*" in
  'view @test/tool versions dist-tags time description homepage --json')
    printf '%s\n' '{"versions":["1.0.0","2.0.0"],"dist-tags":{"latest":"2.0.0"},"description":"Fixture npm","homepage":"https://example.invalid/npm"}' ;;
  'ls --global --depth=0 --json'|'ls --global --depth=0 --json @test/tool')
    printf '%s\n' '{"dependencies":{"@test/tool":{"version":"1.0.0"}}}' ;;
  *) exit 64 ;;
esac
"#,
    );
    executable(
        &bin.join("brew"),
        r#"
case "$*" in
  'info --json=v2 --formula fixture')
    printf '%s\n' '{"formulae":[{"name":"fixture","versions":{"stable":"3.0.0"},"installed":[{"version":"2.0.0"}],"outdated":true,"desc":"Fixture formula","homepage":"https://example.invalid/formula","urls":{"stable":{"url":"https://github.com/acme/fixture/archive/v3.tar.gz"}}}]}' ;;
  'info --json=v2 --cask fixture')
    printf '%s\n' '{"casks":[{"token":"fixture","name":["Fixture App"],"version":"4.0.0","installed":"3.0.0","outdated":true,"desc":"Fixture cask","homepage":"https://example.invalid/cask","url":"https://github.com/acme/fixture/releases/v4.dmg"}]}' ;;
  'info --installed --json=v2')
    printf '%s\n' '{"formulae":[],"casks":[{"token":"fixture","installed":"3.0.0","url":"https://github.com/acme/fixture/releases/v4.dmg"}]}' ;;
  'search --formula fixture'|'search --cask fixture') echo fixture ;;
  *) exit 64 ;;
esac
"#,
    );
    let env = ShellEnv::default()
        .with_var("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .with_var("HOME", home.path().display().to_string())
        .with_var("FIXTURE_HOME", home.path().display().to_string());
    harness_with_env("http://127.0.0.1:9", home, env)
}

pub(crate) fn script_app(id: &str) -> Value {
    json!({
        "id": id, "name": id, "kind": "cli", "auto_update": false,
        "source": "script", "update_command": "printf updated > \"$FIXTURE_HOME/updated\""
    })
}

#[test]
fn matching_and_inventory_share_snake_case_evidence_contract() {
    use super::support::ok;
    let harness = package_harness();
    let brew = ok(
        &harness,
        "match_source",
        json!({"input":"cask:fixture","offline":true}),
    );
    assert_eq!(brew["from"], "brew");
    let repository = &brew["repositories"][0];
    assert_eq!(repository["repo"], "acme/fixture");
    assert_eq!(repository["confidence"], "high");
    assert_eq!(repository["evidence"][0]["kind"], "download_url");
    assert_eq!(repository["tracked_by"], json!([]));
    assert!(repository["description"].is_null());
    assert!(repository["stars"].is_null());
    let github = ok(
        &harness,
        "match_source",
        json!({"input":"acme/fixture","offline":true}),
    );
    assert_eq!(github["from"], "github");
    assert_eq!(github["packages"][0]["package"], "fixture");
    assert_eq!(github["packages"][0]["confidence"], "high");
    assert_eq!(github["packages"][0]["tracked_by"], json!([]));
    assert!(github["errors"].as_array().unwrap().is_empty());
    let inventory = ok(&harness, "discover_apps", json!({"sources":["brew"]}));
    assert_eq!(inventory["candidates"][0]["repositories"][0], *repository);
    assert!(!harness.home.path().join("config/config.toml").exists());
    assert!(!harness.home.path().join("config/state.json").exists());
}
