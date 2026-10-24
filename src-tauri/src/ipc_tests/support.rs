//! Isolated engine and real IPC handler fixtures.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

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

use crate::state::AppState;

pub(crate) struct Harness {
    pub(crate) home: tempfile::TempDir,
    pub(crate) app: App<MockRuntime>,
    pub(crate) window: WebviewWindow<MockRuntime>,
}

pub(crate) fn harness(api: &str) -> Harness {
    harness_with_env(api, tempfile::tempdir().unwrap(), ShellEnv::current())
}

pub(crate) fn harness_with_env(api: &str, home: tempfile::TempDir, env: ShellEnv) -> Harness {
    // A dummy token keeps the keychain and `gh` out of the test.
    let env = env.with_var("HEADWAY_GITHUB_TOKEN", "ghp_dummytokendummytokendummy0000");
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
    Harness { home, app, window }
}

pub(crate) fn call(harness: &Harness, cmd: &str, args: Value) -> Result<Value, Value> {
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

pub(crate) fn ok(harness: &Harness, cmd: &str, args: Value) -> Value {
    let shown = args.to_string();
    call(harness, cmd, args).unwrap_or_else(|err| panic!("{cmd} {shown} failed: {err}"))
}

/// What the add dialog sends for a source build (`appFromForm` output).
pub(crate) fn tool_app(git_url: &str, dest: &Path) -> Value {
    json!({
        "id": "tool",
        "source": "github",
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

pub(super) fn git(dir: &Path, args: &[&str]) -> String {
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

pub(super) fn commit_json(sha: &str) -> Value {
    json!({
        "sha": sha,
        "html_url": format!("https://github.com/octo/tool/commit/{sha}"),
        "commit": { "message": format!("commit {sha}"), "committer": { "date": "2026-10-01T00:00:00Z" } }
    })
}
