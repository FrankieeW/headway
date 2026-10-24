//! Build identity through the real CLI, with an isolated home.
#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use headway_core::Headway;
use serde_json::{Value, json};

#[test]
fn metadata_when_json_is_requested_ignores_runtime_stamps() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(assert_cmd::cargo::cargo_bin!("headway"))
        .env("HEADWAY_HOME", home.path())
        .env(
            "HEADWAY_BUILD_SHA",
            "ffffffffffffffffffffffffffffffffffffffff",
        )
        .env("HEADWAY_BUILD_DIRTY", "true")
        .env("HEADWAY_BUILD_TARGET", "runtime-override")
        .env("HEADWAY_BUILD_REPOSITORY", "runtime-must-not/override")
        .args(["version", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let document: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        document,
        json!({
            "ok": true, "command": "version", "data": Headway::build_info(),
            "next": ["headway commands --json"],
        })
    );
    assert!(output.stderr.is_empty());
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn metadata_when_ai_is_requested_is_one_xml_document() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(assert_cmd::cargo::cargo_bin!("headway"))
        .env("HEADWAY_HOME", home.path())
        .args(["version", "--ai"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    let document = roxmltree::Document::parse(&text).unwrap();
    let root = document.root_element();
    let info = Headway::build_info();
    assert_eq!(root.tag_name().name(), "headway");
    assert_eq!(root.attribute("command"), Some("version"));
    assert_eq!(root.attribute("ok"), Some("true"));
    assert_eq!(root.attribute("version"), Some(info.version.as_str()));
    assert_eq!(root.attribute("commit"), info.commit.as_deref());
    assert_eq!(root.attribute("target"), info.target.as_deref());
    assert_eq!(
        root.attribute("dirty"),
        Some(if info.dirty { "true" } else { "false" })
    );
    assert!(
        root.descendants()
            .any(|node| node.has_tag_name("command")
                && node.text() == Some("headway commands --json"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn version_flag_retains_clap_display_behavior() {
    let home = tempfile::tempdir().unwrap();
    Command::new(assert_cmd::cargo::cargo_bin!("headway"))
        .env("HEADWAY_HOME", home.path())
        .args(["--version", "--json"])
        .assert()
        .success()
        .stdout(format!("headway {}\n", Headway::build_info()))
        .stderr("");
}
