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
use tauri::{App, Manager as _, WebviewWindow, WebviewWindowBuilder};

use super::host::RecordingHost;
use crate::state::AppState;

pub(crate) struct Harness {
    pub(crate) home: tempfile::TempDir,
    pub(crate) app: App<MockRuntime>,
    pub(crate) window: WebviewWindow<MockRuntime>,
    /// Opens links, reveals folders and toggles login items without side effects.
    pub(crate) host: Arc<RecordingHost>,
}

pub(crate) fn harness(api: &str) -> Harness {
    harness_with_env(api, tempfile::tempdir().unwrap(), ShellEnv::current())
}

pub(crate) fn harness_with_env(api: &str, home: tempfile::TempDir, env: ShellEnv) -> Harness {
    // A dummy token keeps the keychain and `gh` out of the test.
    let env = env.with_var("HEADWAY_GITHUB_TOKEN", "ghp_dummytokendummytokendummy0000");
    harness_inner(api, home, env)
}

/// A harness without a token in the environment, for the token commands: the
/// injected [`MemoryStore`](headway_core::token::MemoryStore) is the only secret
/// store, so the login keychain is never touched.
pub(crate) fn harness_without_token(api: &str) -> Harness {
    harness_inner(api, tempfile::tempdir().unwrap(), ShellEnv::current())
}

fn harness_inner(api: &str, home: tempfile::TempDir, env: ShellEnv) -> Harness {
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
    let host = Arc::new(RecordingHost::new());
    app.state::<AppState>().install_host(host.clone());
    let window = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    Harness {
        home,
        app,
        window,
        host,
    }
}

/// A harness with a validated `Headway.app` bundle and `apps_dir`/`bin_dir`
/// inside the temporary home. The bundled CLI answers `--json version` with a
/// clean stamped build, exactly like a real installed copy.
pub(crate) fn bundle_harness() -> Harness {
    let harness = harness("http://127.0.0.1:9");
    let root = harness.home.path().join("apps");
    let contents = root.join("Headway.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::write(
        contents.join("Info.plist"),
        r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>wang.frankie.headway</string><key>CFBundleExecutable</key><string>headway-desktop</string></dict></plist>"#,
    )
    .unwrap();
    let cli = "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"data\":{\"version\":\"9.0.0\",\"commit\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"dirty\":false,\"target\":null}}'\n";
    for name in ["headway", "headway-desktop"] {
        let path = contents.join("MacOS").join(name);
        std::fs::write(&path, cli).unwrap();
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    let state = harness.app.state::<AppState>();
    state
        .hw
        .set_setting("apps_dir", root.to_str().unwrap())
        .unwrap();
    state
        .hw
        .set_setting("bin_dir", harness.home.path().join("bin").to_str().unwrap())
        .unwrap();
    harness
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
