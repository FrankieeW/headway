//! Read-only planning and terminal/error behavior at the binary boundary.
#![allow(clippy::unwrap_used, clippy::panic)]
mod contract_support;
use contract_support::{add, command, decode, result};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[tokio::test]
async fn all_dry_run_is_read_only_even_while_the_app_is_busy() {
    let home = tempfile::tempdir().unwrap();
    add(home.path(), &[]);
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    let commit = json!({"sha":sha,"html_url":"https://github.com/octo/tool/commit/a",
        "commit":{"message":"Fixture","author":{"name":"Test","date":"2026-10-01T00:00:00Z"},
                  "committer":{"name":"Test","date":"2026-10-01T00:00:00Z"}},"author":null});
    for (endpoint, body) in [
        (
            "/repos/octo/tool",
            json!({"full_name":"octo/tool","default_branch":"main","html_url":"https://github.com/octo/tool"}),
        ),
        ("/repos/octo/tool/commits/main", commit.clone()),
        ("/repos/octo/tool/commits", json!([commit])),
    ] {
        Mock::given(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    let lock_path = headway_core::paths::Paths::under(home.path()).app_lock("tool");
    let _held = headway_core::lock::AppLock::acquire(&lock_path, "tool").unwrap();
    let before = snapshot(home.path());
    for args in [
        vec!["update", "--all", "--dry-run"],
        vec!["update", "tool", "--dry-run"],
    ] {
        let planned = decode(
            command(home.path())
                .env("HEADWAY_API_URL", server.uri())
                .arg("--json")
                .args(args)
                .output()
                .unwrap(),
            0,
        );
        assert_eq!(planned["data"]["plans"][0]["app_id"], "tool");
        assert_eq!(
            snapshot(home.path()),
            before,
            "planning must not cache checks or create logs/locks"
        );
    }
    let busy = result(home.path(), &["update", "tool"], 6);
    assert_eq!(busy["error"]["code"], "busy");
    assert_eq!(snapshot(home.path()), before);
}

#[tokio::test]
async fn doctor_roots_match_its_unhealthy_result_in_json_and_xml() {
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/user"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let output = command(home.path())
        .env("PATH", "/headway-test-no-tools")
        .env("HEADWAY_API_URL", server.uri())
        .args(["--json", "doctor"])
        .output()
        .unwrap();
    let document = decode(output, 1);
    assert_eq!(document["data"]["ok"], false);
    let output = command(home.path())
        .env("PATH", "/headway-test-no-tools")
        .env("HEADWAY_API_URL", server.uri())
        .args(["--ai", "doctor"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    let xml = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(xml.root_element().attribute("ok"), Some("false"));
}

#[test]
fn terminal_controls_and_quiet_are_explicit_without_affecting_machine_hints() {
    let home = tempfile::tempdir().unwrap();
    for (name, value) in [("TERM", "dumb"), ("CLICOLOR", "0")] {
        let output = command(home.path())
            .env_remove("NO_COLOR")
            .env("TERM", "xterm")
            .env("CLICOLOR_FORCE", "1")
            .env(name, value)
            .arg("list")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.contains(&0x1b));
    }
    let output = command(home.path())
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env("TERM", "xterm")
        .env("CLICOLOR_FORCE", "1")
        .arg("list")
        .output()
        .unwrap();
    assert!(output.stdout.contains(&0x1b));
    let quiet = command(home.path())
        .args(["--quiet", "list"])
        .output()
        .unwrap();
    assert!(!String::from_utf8(quiet.stdout).unwrap().contains("next:"));
    assert!(quiet.stderr.is_empty());
    let quiet_json = result(home.path(), &["--quiet", "list"], 0);
    assert!(!quiet_json["next"].as_array().unwrap().is_empty());
    result(home.path(), &["--quiet", "--verbose", "list"], 2);
}

#[test]
fn completions_revision_help_and_nested_array_identity_remain_discoverable() {
    let home = tempfile::tempdir().unwrap();
    assert!(result(home.path(), &["completions", "bash"], 0)["data"]["script"].is_string());
    let output = command(home.path())
        .args(["install-commit", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Source build command"));
    assert!(help.contains("Post-install verification"));
    assert!(help.contains("requires --as"));
    add(home.path(), &[]);
    let output = command(home.path())
        .args(["--ai", "repo", "list", "tool"])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let xml = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(
        xml.root_element().attribute("subcommand"),
        Some("repo list")
    );
}
