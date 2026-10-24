use super::*;
use std::os::unix::fs::PermissionsExt as _;

fn fixture() -> (tempfile::TempDir, Headway, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("apps/Headway.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>wang.frankie.headway</string><key>CFBundleExecutable</key><string>headway-desktop</string></dict></plist>"#).unwrap();
    for name in ["headway", "headway-desktop"] {
        let path = app.join("Contents/MacOS").join(name);
        std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"data\":{\"version\":\"0.2.1\",\"commit\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"dirty\":false,\"target\":null}}'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let bin = dir.path().join("bin");
    let hw = Headway::builder()
        .paths(crate::paths::Paths::under(dir.path().join("home")))
        .env(ShellEnv::from_vars([(
            "PATH".into(),
            bin.display().to_string(),
        )]))
        .build()
        .unwrap();
    hw.set_setting("bin_dir", &bin.display().to_string())
        .unwrap();
    hw.set_setting("apps_dir", &dir.path().join("apps").display().to_string())
        .unwrap();
    (dir, hw, app)
}

#[tokio::test]
async fn api_repairs_moved_bundle_and_resolves_path() {
    let (_dir, hw, _app) = fixture();
    let before = hw.cli_status(None).await.unwrap();
    assert!(before.needs_repair);
    let after = hw.fix_cli_link(None).await.unwrap();
    assert!(after.installed && after.on_path);
    assert!(!after.needs_repair && !after.shadowed);
    assert_eq!(
        after.resolved_path.as_deref(),
        Some(after.link_path.as_str())
    );
}

#[tokio::test]
async fn refuses_wrong_bundle_identity() {
    let (_dir, hw, app) = fixture();
    std::fs::write(app.join("Contents/Info.plist"), "invalid").unwrap();
    assert!(hw.fix_cli_link(Some(app.to_str().unwrap())).await.is_err());
}

#[tokio::test]
async fn restart_request_persists_without_modifying_config_or_state() {
    let (_dir, hw, _app) = fixture();
    let config = std::fs::read(hw.paths().config_file()).unwrap();
    let status = hw.request_self_restart().await.unwrap();
    assert!(status.restart_requested && status.pending_restart && !status.confirmed);
    assert!(hw.paths().config_dir.join("self-restart.json").exists());
    assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), config);
    assert!(!hw.paths().state_file().exists());
}

#[tokio::test]
async fn standalone_restart_reports_missing_desktop() {
    let (dir, hw, _app) = fixture();
    hw.set_setting("apps_dir", dir.path().join("absent").to_str().unwrap())
        .unwrap();
    assert!(hw.self_update_status().await.unwrap().installed.is_none());
    assert!(hw.request_self_restart().await.is_err());
}

#[tokio::test]
async fn status_reports_shadowing_without_changing_foreign_binary() {
    let (dir, hw, _app) = fixture();
    hw.fix_cli_link(None).await.unwrap();
    let foreign = dir.path().join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let binary = foreign.join("headway");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hw = Headway::builder()
        .paths(hw.paths().clone())
        .env(ShellEnv::from_vars([(
            "PATH".into(),
            format!("{}:{}", foreign.display(), dir.path().join("bin").display()),
        )]))
        .build()
        .unwrap();
    let status = hw.cli_status(None).await.unwrap();
    assert!(status.installed && status.shadowed && !status.needs_repair);
    assert_eq!(status.resolved_path.unwrap(), binary.display().to_string());
    assert_eq!(
        std::fs::read_to_string(binary).unwrap(),
        "#!/bin/sh\nexit 0\n"
    );
}

#[tokio::test]
async fn failed_startup_confirmation_clears_queued_status() {
    let (_dir, hw, _app) = fixture();
    hw.request_self_restart().await.unwrap();
    let path = hw.paths().config_dir.join("self-restart.json");
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["dispatched"] = serde_json::json!(true);
    // Legacy request files lack the new optional field.
    saved.as_object_mut().unwrap().remove("restart_error");
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(!hw.confirm_self_restart().unwrap());
    let status = hw.self_update_status().await.unwrap();
    assert!(!status.restart_requested);
    assert!(!status.confirmed);
    assert!(status.restart_error.is_some());
    assert!(!hw.take_self_restart().unwrap());
}

#[tokio::test]
async fn fresh_restart_request_clears_previous_confirmation_error() {
    let (_dir, hw, _app) = fixture();
    hw.request_self_restart().await.unwrap();
    let path = hw.paths().config_dir.join("self-restart.json");
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["dispatched"] = serde_json::json!(true);
    saved["restart_error"] = serde_json::json!("previous startup did not match");
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    let status = hw.request_self_restart().await.unwrap();
    assert!(status.restart_requested);
    assert!(!status.confirmed);
    assert!(status.restart_error.is_none());
}
