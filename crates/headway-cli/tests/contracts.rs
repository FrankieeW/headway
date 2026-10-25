//! Regression coverage for review findings, using isolated stores and mocked remotes.
#![allow(clippy::unwrap_used, clippy::panic)]
mod contract_support;
use contract_support::{add, command, decode, result};
use serde_json::Value;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

#[test]
fn closed_stdout_is_quiet_and_preserves_the_command_status() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    let home = tempfile::tempdir().unwrap();
    for (args, code) in [
        (vec!["--json", "commands"], 0),
        (vec!["--ai", "commands"], 0),
        (vec!["commands"], 0),
        (vec!["--json", "bogus"], 2),
        (vec!["--ai", "bogus"], 2),
        (vec!["--json", "show", "missing"], 3),
        (vec!["--help"], 0),
    ] {
        // The consumer is already closed before the child starts: no timing race.
        let (reader, writer) = UnixStream::pair().unwrap();
        drop(reader);
        let output = command(home.path())
            .args(&args)
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
    }
}

#[test]
fn no_inspect_release_asset_saves_without_a_placeholder_source_recipe() {
    let home = tempfile::tempdir().unwrap();
    let args = [
        "add",
        "octo/tool",
        "--no-inspect",
        "--track",
        "release",
        "--method",
        "asset",
        "--pattern",
        "*.tar.gz",
    ];
    let mut preview = args.to_vec();
    preview.push("--dry-run");
    let planned = result(home.path(), &preview, 0);
    assert_eq!(planned["data"]["saved"], false);
    assert!(!home.path().join("config/config.toml").exists());
    let saved = result(home.path(), &args, 0);
    assert_eq!(saved["data"]["saved"], true);
    assert_eq!(saved["data"]["app"]["install"]["method"], "asset");
    assert!(saved["data"]["app"]["options"]["source_build"].is_null());
    assert_eq!(
        result(home.path(), &["show", "tool"], 0)["data"]["app"],
        saved["data"]["app"]
    );
}

#[test]
fn dry_run_and_real_add_edit_reject_the_same_candidates_without_writing() {
    let home = tempfile::tempdir().unwrap();
    add(home.path(), &[]);
    let path = home.path().join("config/config.toml");
    let before = std::fs::read(&path).unwrap();
    let dest = tempfile::tempdir().unwrap();
    let dest = dest.path().to_str().unwrap();
    for args in [
        vec![
            "add",
            "other/repo",
            "--id",
            "tool",
            "--no-inspect",
            "--build",
            "true",
        ],
        vec![
            "add",
            "other/repo",
            "--no-inspect",
            "--build",
            "true",
            "--dest",
            dest,
        ],
        vec!["edit", "tool", "--dest", dest],
    ] {
        let mut preview = args.clone();
        preview.push("--dry-run");
        let planned = result(home.path(), &preview, 2);
        let real = result(home.path(), &args, 2);
        assert_eq!(planned["error"], real["error"]);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[tokio::test]
async fn partial_failures_have_honest_roots_and_per_item_exit_codes() {
    let home = tempfile::tempdir().unwrap();
    add(home.path(), &["--auto-update"]);
    add(home.path(), &["--id", "second", "--auto-update"]);
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    for args in [
        vec!["check"],
        vec!["update", "--all"],
        vec!["update", "tool", "second"],
        vec!["auto-update"],
    ] {
        let value = decode(
            command(home.path())
                .env("HEADWAY_API_URL", server.uri())
                .arg("--json")
                .args(&args)
                .output()
                .unwrap(),
            4,
        );
        assert!(!value["data"]["results"].as_array().unwrap().is_empty());
        assert_eq!(value["data"]["results"][0]["ok"], false);
        assert_eq!(value["data"]["results"][0]["error"]["exit_code"], 4);
        if args == ["check"] {
            assert!(!value["next"].as_array().unwrap().is_empty());
        }
    }
    let output = command(home.path())
        .env("HEADWAY_API_URL", server.uri())
        .args(["--ai", "check"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4));
    let text = String::from_utf8(output.stdout).unwrap();
    let xml = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(xml.root_element().attribute("ok"), Some("false"));
}

#[test]
fn invalid_argument_values_are_not_misreported_as_corrupt_files() {
    let home = tempfile::tempdir().unwrap();
    result(home.path(), &["import", "recipe:pnpm-npm"], 0);
    for args in [
        vec![
            "add",
            "octo/tool",
            "--no-inspect",
            "--build",
            "true",
            "--id",
            "bad id",
        ],
        vec!["edit", "pnpm-npm", "--dist-tag", ""],
        vec!["edit", "pnpm-npm", "--package", "BAD PACKAGE!"],
        vec!["edit", "pnpm-npm", "--name", ""],
    ] {
        let value = result(home.path(), &args, 2);
        assert_eq!(value["error"]["code"], "invalid_input");
        assert!(
            !value["error"]["hint"]
                .as_str()
                .unwrap_or_default()
                .contains("fix the file")
        );
    }
    std::fs::write(home.path().join("config/config.toml"), "invalid = [").unwrap();
    let value = result(home.path(), &["list"], 1);
    assert_eq!(value["error"]["code"], "parse_error");
}

#[test]
fn output_environment_and_explicit_precedence_are_predictable() {
    let home = tempfile::tempdir().unwrap();
    decode(
        command(home.path())
            .env("HEADWAY_OUTPUT", "")
            .args(["--json", "list"])
            .output()
            .unwrap(),
        0,
    );
    decode(
        command(home.path())
            .env("HEADWAY_OUTPUT", "json")
            .arg("list")
            .output()
            .unwrap(),
        0,
    );
    decode(
        command(home.path())
            .env("HEADWAY_OUTPUT", "xml")
            .args(["--output", "human", "--json", "list"])
            .output()
            .unwrap(),
        0,
    );
    let output = command(home.path())
        .args(["--json", "--ai", "list"])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        roxmltree::Document::parse(&text)
            .unwrap()
            .root_element()
            .attribute("ok"),
        Some("true")
    );
    let help = command(home.path())
        .args(["--json", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("Usage:"));
    let unknown = result(home.path(), &["bogus"], 2);
    assert_eq!(unknown["command"], "bogus");
}

#[test]
fn command_metadata_expresses_globals_conflicts_requirements_and_boolean_arity() {
    let home = tempfile::tempdir().unwrap();
    let document = result(home.path(), &["commands"], 0);
    let tree = &document["data"];
    let sub = |name: &str| {
        tree["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sub| sub["name"] == name)
            .unwrap()
    };
    let arg = |sub: &Value, name: &str| {
        sub["args"]
            .as_array()
            .unwrap()
            .iter()
            .find(|arg| arg["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(arg(sub("list"), "json")["global"], true);
    assert_eq!(arg(sub("update"), "ids")["conflicts_with"][0], "all");
    assert_eq!(
        arg(sub("install-commit"), "name")["requires"][0],
        "variant_id"
    );
    let optional = arg(sub("add"), "prerelease");
    assert_eq!(optional["num_args"]["min"], 0);
    assert_eq!(optional["num_args"]["max"], 1);
    assert_eq!(optional["default_missing_value"], "true");
    assert_eq!(
        optional["possible_values"],
        serde_json::json!(["true", "false"])
    );
    assert_eq!(arg(sub("add"), "keep_source")["num_args"]["min"], 1);
    assert!(arg(sub("add"), "keep_source")["default_missing_value"].is_null());
    result(
        home.path(),
        &["install-commit", "tool", "main", "--name", "variant"],
        2,
    );
    let human = command(home.path()).arg("commands").output().unwrap();
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("Global flags"));
    assert!(human.contains("token status"));
}

#[test]
fn next_hints_never_fabricate_a_baseline_and_keep_the_requested_format() {
    let home = tempfile::tempdir().unwrap();
    let added = add(home.path(), &[]);
    for value in [added, result(home.path(), &["show", "tool"], 0)] {
        for hint in value["next"].as_array().unwrap() {
            let hint = hint.as_str().unwrap();
            assert!(!hint.contains(" mark "));
            assert!(hint.starts_with("headway --json "), "{hint}");
        }
    }
    let storage = result(home.path(), &["source-storage", "tool"], 0);
    assert!(!storage["next"].as_array().unwrap().is_empty());
    result(home.path(), &["edit", "tool", "--keep-source", "false"], 0);
    assert_eq!(
        result(home.path(), &["source-storage", "tool"], 0)["next"],
        serde_json::json!([])
    );
    let fresh = tempfile::tempdir().unwrap();
    assert!(
        !result(fresh.path(), &["update", "--all"], 0)["next"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn token_verification_failure_suggests_retry_not_replacing_a_configured_token() {
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "9999999999"),
        )
        .mount(&server)
        .await;
    let status = decode(
        command(home.path())
            .env("HEADWAY_API_URL", server.uri())
            .args(["--json", "token", "status"])
            .output()
            .unwrap(),
        0,
    );
    assert!(status["data"]["masked"].is_string());
    assert_eq!(status["next"][0], "headway --json token status");
    assert_eq!(status["command"], "token");
    assert_eq!(status["subcommand"], "token status");
}

#[test]
fn purge_preview_lists_backups_and_no_backups_has_human_output() {
    let home = tempfile::tempdir().unwrap();
    add(home.path(), &["--artifact", "out/tool"]);
    let preview = result(home.path(), &["remove", "tool", "--purge", "--dry-run"], 0);
    assert!(
        preview["data"]["would_remove"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path.as_str().unwrap().ends_with("backups/tool"))
    );
    let output = command(home.path())
        .args(["backups", "tool"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No retained backups")
    );
}

#[test]
fn xml_repository_entities_have_consistent_singular_names() {
    let home = tempfile::tempdir().unwrap();
    add(home.path(), &[]);
    result(home.path(), &["repo", "add", "tool", "octo/fork"], 0);
    for args in [
        ["show", "tool"].as_slice(),
        ["repo", "list", "tool"].as_slice(),
    ] {
        let output = command(home.path())
            .arg("--ai")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let xml = roxmltree::Document::parse(&text).unwrap();
        assert!(
            xml.descendants()
                .any(|node| node.has_tag_name("repository")),
            "{text}"
        );
        assert!(!text.contains("<repositorie "));
    }
}

#[test]
fn pagination_and_token_stdin_errors_are_safe_before_any_remote_or_keychain_write() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        ["prs", "tool", "--page", "0"],
        ["commits", "tool", "--per-page", "101"],
    ] {
        assert_eq!(
            result(home.path(), &args, 2)["error"]["code"],
            "usage_error"
        );
    }
    for args in [vec!["token", "set"], vec!["token", "set", "--stdin"]] {
        let output = command(home.path())
            .arg("--json")
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert_eq!(decode(output, 2)["error"]["code"], "invalid_input");
    }
}
