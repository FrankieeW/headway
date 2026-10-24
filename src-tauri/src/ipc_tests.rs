//! The window's IPC contract: commands invoked with exactly the JSON the
//! TypeScript layer (`src/lib/api/index.ts`) sends, through the real handler.

mod build_info;
mod operations;
pub(crate) mod packages;
mod sources;
pub(crate) mod support;

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::{call, commit_json, git, harness, ok, tool_app};

#[test]
fn app_and_settings_commands_accept_the_frontend_payloads() {
    let harness = harness("http://127.0.0.1:9");
    assert_eq!(ok(&harness, "list_apps", json!({})), json!([]));

    let dest = harness.home.path().join("bin/tool");
    let added = ok(
        &harness,
        "add_app",
        json!({ "app": tool_app("file:///nowhere", &dest) }),
    );
    assert_eq!(added["status"], "unchecked");
    assert_eq!(added["app"]["repo"], "octo/tool", "URLs are normalized");

    let mut edited = tool_app("file:///nowhere", &dest);
    edited["name"] = json!("Renamed");
    assert_eq!(
        ok(&harness, "save_app", json!({ "app": edited }))["app"]["name"],
        "Renamed"
    );
    assert_eq!(
        ok(&harness, "get_app", json!({ "id": "tool" }))["app"]["name"],
        "Renamed"
    );
    assert_eq!(
        ok(&harness, "app_history", json!({ "id": "tool" })),
        json!([])
    );
    assert_eq!(
        ok(&harness, "read_log", json!({ "id": "tool" })),
        Value::Null
    );
    assert!(
        ok(&harness, "list_presets", json!({}))
            .as_array()
            .unwrap()
            .len()
            > 5
    );

    let settings = ok(&harness, "get_settings", json!({}));
    assert_eq!(settings["check_interval_minutes"], 60);
    let mut changed = settings.clone();
    changed["check_interval_minutes"] = json!(15);
    assert_eq!(
        ok(&harness, "save_settings", json!({ "settings": changed }))["check_interval_minutes"],
        15
    );
    assert!(
        ok(&harness, "get_paths", json!({}))["config_file"]
            .as_str()
            .unwrap()
            .ends_with("config.toml")
    );

    // Errors arrive as `{code, message, hint}`.
    let invalid = call(
        &harness,
        "add_app",
        json!({ "app": tool_app("file:///nowhere", &dest) }),
    )
    .unwrap_err();
    assert_eq!(invalid["code"], "app_exists");
    assert!(invalid["hint"].is_string());

    // Nothing is running for an idle app.
    assert_eq!(
        ok(&harness, "cancel_update", json!({ "id": "tool" })),
        false
    );

    assert_eq!(
        ok(
            &harness,
            "remove_app",
            json!({ "id": "tool", "purge": true })
        )["purged"],
        true
    );
    let missing = call(&harness, "get_app", json!({ "id": "tool" })).unwrap_err();
    assert_eq!(missing["code"], "app_not_found");
    let unknown = call(&harness, "cancel_update", json!({ "id": "tool" })).unwrap_err();
    assert_eq!(unknown["code"], "app_not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn check_plan_mark_and_update_through_ipc() {
    let upstream = tempfile::tempdir().unwrap();
    git(upstream.path(), &["init", "-q", "-b", "main"]);
    let mut shas = Vec::new();
    for version in ["v1", "v2"] {
        std::fs::write(upstream.path().join("VERSION"), version).unwrap();
        git(upstream.path(), &["add", "-A"]);
        git(upstream.path(), &["commit", "-q", "-m", version]);
        shas.push(git(upstream.path(), &["rev-parse", "HEAD"]));
    }
    let server = MockServer::start().await;
    let respond = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    Mock::given(path("/repos/octo/tool"))
        .respond_with(respond(json!({ "full_name": "octo/tool", "default_branch": "main", "html_url": "https://github.com/octo/tool" })))
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/commits/main"))
        .respond_with(respond(commit_json(&shas[1])))
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/commits"))
        .respond_with(respond(json!([
            commit_json(&shas[1]),
            commit_json(&shas[0])
        ])))
        .mount(&server)
        .await;
    for sha in &shas {
        Mock::given(path(format!("/repos/octo/tool/commits/{sha}")))
            .respond_with(respond(commit_json(sha)))
            .mount(&server)
            .await;
    }
    Mock::given(path(format!(
        "/repos/octo/tool/compare/{}...{}",
        shas[0], shas[1]
    )))
    .respond_with(respond(
        json!({ "status": "ahead", "ahead_by": 1, "commits": [commit_json(&shas[1])] }),
    ))
    .mount(&server)
    .await;

    let harness = harness(&server.uri());
    let dest = harness.home.path().join("bin/tool");
    let git_url = format!("file://{}", upstream.path().display());
    ok(
        &harness,
        "add_app",
        json!({ "app": tool_app(&git_url, &dest) }),
    );

    let checked = ok(&harness, "check_app", json!({ "id": "tool" }));
    assert_eq!(checked["status"], "not_installed");

    let marked = ok(
        &harness,
        "mark_installed",
        json!({ "id": "tool", "reference": shas[0] }),
    );
    assert_eq!(marked["installed"]["id"], shas[0]);

    let plan = ok(
        &harness,
        "plan_update",
        json!({ "id": "tool", "target": "next" }),
    );
    assert_eq!(plan["to"]["id"], shas[1]);
    assert!(!plan["steps"].as_array().unwrap().is_empty());

    assert_eq!(
        ok(
            &harness,
            "start_update",
            json!({ "id": "tool", "target": { "ref": shas[1] } })
        ),
        Value::Null
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    let installed = loop {
        let view = ok(&harness, "get_app", json!({ "id": "tool" }));
        if view["installed"]["id"] == shas[1] && view["busy"] == false {
            break view;
        }
        assert!(
            view["last_error"].is_null(),
            "update failed: {}",
            view["last_error"]
        );
        assert!(Instant::now() < deadline, "update did not finish: {view}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(installed["status"], "up_to_date");
    let output = Command::new(&dest).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "v2");

    let history = ok(&harness, "app_history", json!({ "id": "tool" }));
    assert_eq!(history[0]["action"], "update");
    assert_eq!(history[1]["action"], "mark");
    let log = ok(&harness, "read_log", json!({ "id": "tool" }));
    assert!(
        log["content"]
            .as_str()
            .unwrap()
            .contains("Running `mkdir -p out")
    );

    let summary = ok(&harness, "check_all", json!({}));
    assert_eq!(summary["checked"], 1);
}
