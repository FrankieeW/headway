//! The window's IPC contract: commands invoked with exactly the JSON the
//! TypeScript layer (`src/lib/api/index.ts`) sends, through the real handler.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use headway_core::Headway;
use headway_core::paths::Paths;
use headway_core::shell::ShellEnv;
use headway_core::token::MemoryStore;
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::state::AppState;

struct Harness {
    home: tempfile::TempDir,
    _app: App<MockRuntime>,
    window: WebviewWindow<MockRuntime>,
}

fn harness(api: &str) -> Harness {
    let home = tempfile::tempdir().unwrap();
    // A dummy token keeps the keychain and `gh` out of the test.
    let env =
        ShellEnv::current().with_var("HEADWAY_GITHUB_TOKEN", "ghp_dummytokendummytokendummy0000");
    let hw = Headway::builder()
        .paths(Paths::under(home.path()))
        .secrets(Arc::new(MemoryStore::default()))
        .env(env)
        .api(api)
        .build()
        .unwrap();
    let app = mock_builder()
        .manage(AppState::new(hw))
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    Harness {
        home,
        _app: app,
        window,
    }
}

fn call(harness: &Harness, cmd: &str, args: Value) -> Result<Value, Value> {
    let request = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        // The window's own (local) origin, as for the real UI.
        url: harness.window.url().unwrap(),
        body: InvokeBody::Json(args),
        headers: tauri::http::HeaderMap::default(),
        invoke_key: INVOKE_KEY.into(),
    };
    get_ipc_response(&harness.window, request).map(|body| body.deserialize::<Value>().unwrap())
}

fn ok(harness: &Harness, cmd: &str, args: Value) -> Value {
    let shown = args.to_string();
    call(harness, cmd, args).unwrap_or_else(|err| panic!("{cmd} {shown} failed: {err}"))
}

/// What the add dialog sends for a source build (`appFromForm` output).
fn tool_app(git_url: &str, dest: &Path) -> Value {
    json!({
        "id": "tool",
        "name": "Tool",
        "repo": "https://github.com/octo/tool",
        "kind": "cli",
        "auto_update": false,
        "git_url": git_url,
        "track": { "mode": "commit", "branch": null },
        "install": {
            "method": "source",
            "build": "mkdir -p out && printf '#!/bin/sh\\necho %s\\n' \"$(cat VERSION)\" > out/tool && chmod +x out/tool",
            "workdir": null,
            "artifact": "out/tool",
            "dest": dest.display().to_string()
        }
    })
}

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

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@headway.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit_json(sha: &str) -> Value {
    json!({
        "sha": sha,
        "html_url": format!("https://github.com/octo/tool/commit/{sha}"),
        "commit": { "message": format!("commit {sha}"), "committer": { "date": "2026-10-01T00:00:00Z" } }
    })
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
