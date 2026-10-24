//! Offline source workflows through the real CLI and stateful executable fixtures.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::fs;

mod source_support;
use source_support::Sandbox;

#[test]
fn npm_detects_installs_plans_steps_updates_and_rolls_back() {
    let s = Sandbox::new();
    let found = s.ok(&["inspect", "npm:@acme/tool"]);
    assert_eq!(found["data"]["installed"], "1.0.0");
    s.ok(&["add", "npm:@acme/tool"]);
    let checked = s.ok(&["check", "tool"]);
    assert_eq!(checked["data"]["results"][0]["pending_count"], 2);
    let plan = s.ok(&["update", "tool", "--next", "--dry-run"]);
    assert_eq!(plan["data"]["plans"][0]["to"]["id"], "1.1.0");
    s.unchanged();
    s.ok(&["update", "tool", "--next"]);
    assert_eq!(
        fs::read_to_string(s.dir.path().join("npm-version")).unwrap(),
        "1.1.0"
    );
    s.ok(&["update", "tool", "--to", "1.0.0"]);
    assert_eq!(
        fs::read_to_string(s.dir.path().join("npm-version")).unwrap(),
        "1.0.0"
    );
    assert_eq!(s.run(&["mark", "tool"])["ok"], false);
    s.ok(&["edit", "tool", "--dist-tag", "beta"]);
    assert_eq!(
        s.ok(&["check", "tool"])["data"]["results"][0]["latest"]["id"],
        "1.1.0"
    );
}

#[test]
fn brew_formula_and_cask_update_but_reject_version_selection() {
    for source in ["brew:tool", "cask:tool"] {
        let s = Sandbox::new();
        s.ok(&["add", source]);
        assert_eq!(
            s.ok(&["check", "tool"])["data"]["results"][0]["status"],
            "update_available"
        );
        s.ok(&["update", "tool", "--dry-run"]);
        for flag in [vec!["--next"], vec!["--to", "1.0.0"]] {
            let mut args = vec!["update", "tool", "--dry-run"];
            args.extend(flag);
            assert_eq!(s.run(&args)["ok"], false);
        }
        assert_eq!(s.run(&["mark", "tool"])["ok"], false);
        s.unchanged();
        s.ok(&["update", "tool"]);
        assert_eq!(
            fs::read_to_string(s.dir.path().join("brew-version")).unwrap(),
            "2.0.0"
        );
        assert_eq!(
            s.ok(&["check", "tool"])["data"]["results"][0]["status"],
            "up_to_date"
        );
        let mutations = fs::read_to_string(s.dir.path().join("mutations")).unwrap();
        assert!(mutations.contains(if source.starts_with("cask:") {
            "--cask"
        } else {
            "--formula"
        }));
    }
}

#[test]
fn script_unknown_status_dry_run_and_real_update() {
    let s = Sandbox::new();
    s.ok(&[
        "add",
        "script:official",
        "--update-command",
        "printf 2.0.0 > \"$HEADWAY_HOME/mutations\"",
    ]);
    let checked = s.ok(&["check", "official"]);
    assert_eq!(checked["data"]["results"][0]["status"], "unknown");
    assert!(!checked["next"].to_string().contains("mark"));
    s.ok(&["update", "official", "--dry-run"]);
    for args in [
        vec!["update", "official", "--to", "1.0.0"],
        vec!["update", "official", "--next"],
        vec!["mark", "official"],
    ] {
        assert_eq!(s.run(&args)["ok"], false);
    }
    s.unchanged();
    s.ok(&["update", "official"]);
    assert_eq!(
        fs::read_to_string(s.dir.path().join("mutations")).unwrap(),
        "2.0.0"
    );
    s.ok(&[
        "edit",
        "official",
        "--version-command",
        "cat \"$HEADWAY_HOME/mutations\"",
        "--latest-command",
        "echo 3.0.0",
    ]);
    assert_eq!(
        s.ok(&["check", "official"])["data"]["results"][0]["status"],
        "update_available"
    );
    s.ok(&["edit", "official", "--latest-command", ""]);
    assert_eq!(
        s.ok(&["check", "official"])["data"]["results"][0]["status"],
        "unknown"
    );
}

#[test]
fn no_inspect_defaults_irrelevant_flags_and_xml_contract() {
    let s = Sandbox::new();
    for (source, expected) in [
        ("npm:@acme/tool", "npm"),
        ("brew:tool", "brew"),
        ("cask:tool", "brew"),
        ("octo/tool", "github"),
    ] {
        let args = [
            "add",
            source,
            "--no-inspect",
            "--dry-run",
            "--build",
            "true",
        ];
        let args = if expected == "github" {
            &args[..]
        } else {
            &args[..4]
        };
        let result = s.ok(args);
        assert_eq!(result["data"]["app"]["source"], expected);
        assert_eq!(result["data"]["saved"], false);
    }
    s.ok(&[
        "add",
        "script:offline",
        "--no-inspect",
        "--update-command",
        "true",
        "--dry-run",
    ]);
    for args in [
        vec!["add", "npm:tool", "--build", "true"],
        vec!["add", "brew:tool", "--dist-tag", "beta"],
        vec!["add", "script:tool", "--cask"],
        vec!["add", "octo/tool", "--update-command", "true"],
    ] {
        let result = s.run(&args);
        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "invalid_input");
    }
    let output = s
        .command()
        .args(["--ai", "add", "npm:tool", "--no-inspect", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let xml = String::from_utf8(output.stdout).unwrap();
    roxmltree::Document::parse(&xml).unwrap();
    if let Ok(root) = std::env::var("HEADWAY_TEST_EVIDENCE") {
        fs::write(std::path::Path::new(&root).join("no-inspect.xml"), &xml).unwrap();
    }

    s.unchanged();
}

#[test]
fn legacy_github_config_reads_and_roundtrips_with_source_discriminator() {
    let s = Sandbox::new();
    fs::create_dir(s.dir.path().join("config")).unwrap();
    fs::write(
        s.dir.path().join("config/config.toml"),
        r#"
[[app]]
id = "legacy"
name = "Legacy"
repo = "octo/tool"
kind = "cli"
[app.track]
mode = "commit"
[app.install]
method = "source"
build = "true"
"#,
    )
    .unwrap();
    let shown = s.ok(&["show", "legacy"]);
    assert_eq!(shown["data"]["app"]["source"], "github");
    s.ok(&["edit", "legacy", "--name", "Renamed"]);
    assert_eq!(
        s.ok(&["show", "legacy"])["data"]["app"]["repo"],
        "octo/tool"
    );
    assert!(
        fs::read_to_string(s.dir.path().join("config/config.toml"))
            .unwrap()
            .contains("source = \"github\"")
    );
}
