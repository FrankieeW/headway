//! No-network `ensure_self_subscription` tests for legacy/missing build metadata.
#![allow(clippy::unwrap_used)] // isolated fixture setup must fail loudly

use super::*;
use crate::AppStatus;
use std::os::unix::fs::PermissionsExt as _;

/// A validated Headway bundle whose installed CLI prints `probe` for `--json version`.
/// The API points at a closed port so any HTTP attempt fails immediately: the flow under
/// test must register the subscription from local evidence alone.
fn fixture(probe: &str) -> (tempfile::TempDir, Headway) {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("apps/Headway.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(
        app.join("Contents/Info.plist"),
        r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>wang.frankie.headway</string><key>CFBundleExecutable</key><string>headway-desktop</string></dict></plist>"#,
    )
    .unwrap();
    for name in ["headway", "headway-desktop"] {
        let path = app.join("Contents/MacOS").join(name);
        std::fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' '{probe}'\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let hw = Headway::builder()
        .paths(crate::paths::Paths::under(root.path().join("home")))
        .api("http://127.0.0.1:1")
        .build()
        .unwrap();
    hw.set_setting("apps_dir", root.path().join("apps").to_str().unwrap())
        .unwrap();
    (root, hw)
}

/// Metadata written before build stamps existed: no commit, no repository, no target.
const LEGACY_WITHOUT_COMMIT: &str =
    r#"{"ok":true,"data":{"version":"0.2.0","commit":null,"dirty":false,"target":null}}"#;

#[tokio::test]
async fn missing_build_metadata_registers_the_subscription_without_a_baseline_or_network() {
    let (_root, hw) = fixture(LEGACY_WITHOUT_COMMIT);
    let view = hw.ensure_self_subscription().await.unwrap();
    assert_eq!(view.app.id, "headway");
    assert_eq!(view.app.source.kind(), "github");
    assert!(!view.app.auto_update);
    // No commit means no provable baseline: never invent one.
    assert!(view.installed.is_none());
    assert_eq!(view.status, AppStatus::Unknown);
    assert_eq!(hw.list().unwrap().len(), 1);
}

#[tokio::test]
async fn rejected_version_probe_is_a_typed_invalid_input_error() {
    let (_root, hw) = fixture(
        r#"{"ok":false,"data":{"version":"0.2.0","commit":null,"dirty":false,"target":null}}"#,
    );
    let err = hw.ensure_self_subscription().await.unwrap_err();
    assert!(
        matches!(&err, Error::InvalidInput(message) if message.contains("rejected version probe"))
    );
    assert_eq!(err.code(), "invalid_input");
    // The probe runs before registration, so a rejected build leaves no half-registered app.
    assert!(hw.list().unwrap().is_empty());
}
