//! CLI contracts for retention, repository choices and independent commit previews.
#![allow(clippy::unwrap_used)]
use assert_cmd::Command;
use serde_json::{Value, json};

fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("headway"));
    command
        .env("HEADWAY_HOME", root)
        .env_remove("HEADWAY_OUTPUT")
        .env("NO_COLOR", "1");
    command
}
fn document(root: &std::path::Path, args: &[&str]) -> Value {
    let output = command(root)
        .arg("--json")
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn saved_storage_and_official_fork_choices_roundtrip_without_building() {
    let root = tempfile::tempdir().unwrap();
    document(root.path(), &["import", "recipe:headway-app"]);
    let storage = root.path().join("retained sources");
    let changed = document(
        root.path(),
        &[
            "edit",
            "headway-app",
            "--keep-source",
            "false",
            "--source-root",
            storage.to_str().unwrap(),
        ],
    );
    assert_eq!(changed["data"]["app"]["install"]["storage"]["keep"], false);
    let status = document(root.path(), &["source-storage", "headway-app"]);
    assert_eq!(status["data"]["custom_root"], true);
    assert_eq!(status["data"]["exists"], false);
    assert!(!storage.exists());
    document(
        root.path(),
        &[
            "edit",
            "headway-app",
            "--method",
            "asset",
            "--pattern",
            "*.dmg",
        ],
    );
    let renamed = document(
        root.path(),
        &["edit", "headway-app", "--name", "Headway download"],
    );
    assert_eq!(renamed["data"]["app"]["install"]["method"], "asset");
    let restored = document(root.path(), &["edit", "headway-app", "--method", "source"]);
    assert_eq!(restored["data"]["app"]["install"]["storage"]["keep"], false);
    assert_eq!(
        restored["data"]["app"]["install"]["artifact"],
        ".headway-out/Headway.app"
    );
    document(
        root.path(),
        &[
            "repo",
            "add",
            "headway-app",
            "my-fork/headway",
            "--label",
            "My fork",
            "--branch",
            "work",
        ],
    );
    let choices = document(root.path(), &["repo", "list", "headway-app"]);
    assert_eq!(choices["data"].as_array().unwrap().len(), 2);
    let switched = document(
        root.path(),
        &["repo", "use", "headway-app", "my-fork/headway"],
    );
    assert_eq!(switched["data"]["app"]["repo"], "my-fork/headway");
    assert_eq!(switched["data"]["app"]["track"]["branch"], "work");
    assert_eq!(switched["data"]["app"]["install"]["storage"]["keep"], false);
    document(
        root.path(),
        &["repo", "use", "headway-app", "FrankieeW/headway"],
    );
    document(
        root.path(),
        &["repo", "remove", "headway-app", "my-fork/headway"],
    );
    let output = command(root.path())
        .args(["--ai", "repo", "list", "headway-app"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        roxmltree::Document::parse(&text)
            .unwrap()
            .root_element()
            .attribute("command"),
        Some("repo")
    );
}

#[test]
fn source_storage_flags_and_commit_browsing_reject_package_manager_sources() {
    let root = tempfile::tempdir().unwrap();
    document(root.path(), &["import", "recipe:pnpm-npm"]);
    let before = std::fs::read(root.path().join("config/config.toml")).unwrap();
    for args in [
        vec!["edit", "pnpm-npm", "--keep-source", "false"],
        vec!["commits", "pnpm-npm"],
    ] {
        let output = command(root.path())
            .arg("--json")
            .args(args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["error"]["code"], "invalid_input");
    }
    assert_eq!(
        std::fs::read(root.path().join("config/config.toml")).unwrap(),
        before
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn selected_commit_preview_has_independent_destination_and_saves_nothing() {
    use wiremock::matchers::{path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let root = tempfile::tempdir().unwrap();
    document(root.path(), &["import", "recipe:headway-app"]);
    document(
        root.path(),
        &[
            "settings",
            "set",
            "apps_dir",
            root.path().join("apps").to_str().unwrap(),
        ],
    );
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    let commit = json!({"sha":sha,"html_url":"https://github.com/FrankieeW/headway/commit/a","commit":{"message":"picked","committer":{"date":"2026-10-01T00:00:00Z"}}});
    Mock::given(path(format!("/repos/FrankieeW/headway/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit.clone()))
        .mount(&server)
        .await;
    Mock::given(path("/repos/FrankieeW/headway/commits"))
        .and(query_param("page", "2"))
        .and(query_param("sha", &sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([commit])))
        .mount(&server)
        .await;
    for endpoint in ["tags", "releases"] {
        Mock::given(path(format!("/repos/FrankieeW/headway/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
    }
    let before = std::fs::read(root.path().join("config/config.toml")).unwrap();
    let invoke = |args: &[&str]| {
        let output = command(root.path())
            .env("HEADWAY_API_URL", server.uri())
            .env("HEADWAY_GITHUB_TOKEN", "dummy-test-token")
            .arg("--json")
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let page = invoke(&["commits", "headway-app", "--page", "2", "--anchor", &sha]);
    assert_eq!(page["data"]["anchor"], sha);
    assert_eq!(page["data"]["page"], 2);
    let plan = invoke(&[
        "install-commit",
        "headway-app",
        &sha,
        "--as",
        "headway-old",
        "--dry-run",
    ]);
    assert_eq!(plan["data"]["app_id"], "headway-old");
    assert!(
        plan["data"]["steps"]
            .to_string()
            .contains("Headway Versions/headway-old.app")
    );
    assert_eq!(
        std::fs::read(root.path().join("config/config.toml")).unwrap(),
        before
    );
    assert_eq!(document(root.path(), &["list"])["data"]["total"], 1);
}
