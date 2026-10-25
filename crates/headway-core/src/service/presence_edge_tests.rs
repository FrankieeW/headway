use super::*;
use crate::service::{AppStatus, AppView};
use crate::state::AppState;
use crate::testing::{cli_app, with_github};
use chrono::Utc;

fn configured(path: &Path) -> TrackedApp {
    with_github(cli_app("tool"), |mut github| {
        github.install = Install::Source(crate::SourceInstall {
            build: "true".into(),
            dest: Some(path.display().to_string()),
            verify_command: Some("exit 99".into()),
            ..crate::SourceInstall::default()
        });
        github
    })
}

#[test]
fn permission_denied_metadata_is_an_error_not_missing() {
    // Given the filesystem's permission-denied response, independent of test-user privileges.
    let error = std::io::Error::from(ErrorKind::PermissionDenied);
    // When classifying the same metadata result used by the real path probe.
    let result = metadata_result(Path::new("/denied/payload"), Err(error));
    // Then uncertainty cannot become an absent payload or authorize reinstall.
    assert_eq!(result.unwrap_err().code(), "io_error");
}

#[test]
fn uninspectable_path_propagates_to_view_and_baseline_selection() {
    // Given a path traversing a regular file, producing ENOTDIR rather than NotFound.
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("file");
    std::fs::write(&parent, "not a directory").unwrap();
    let app = configured(&parent.join("payload"));
    // When observing it for view and update-baseline selection.
    let view = AppView::new(app.clone(), AppState::default(), false);
    let baseline = retained(&app, None);
    // Then both surface the I/O failure instead of claiming absence.
    assert_eq!(view.status, AppStatus::Error);
    assert_eq!(baseline.unwrap_err().code(), "io_error");
}

#[test]
fn recorded_paths_detect_removal_without_a_configured_destination() {
    // Given an app with only a recorded managed path.
    let root = tempfile::tempdir().unwrap();
    let installed = InstalledRef {
        id: "abc1234".into(),
        sha: Some("abc1234".into()),
        version: None,
        installed_at: Utc::now(),
        path: Some(root.path().join("removed").display().to_string()),
    };
    let app = cli_app("tool");
    // When reconciling the baseline and view.
    let baseline = retained(&app, Some(installed.clone())).unwrap();
    let view = AppView::new(
        app,
        AppState {
            installed: Some(installed),
            ..AppState::default()
        },
        false,
    );
    // Then a missing recorded payload cannot be reported as installed.
    assert!(baseline.is_none());
    assert!(view.installed.is_none());
    assert_eq!(view.status, AppStatus::NotInstalled);
}

#[test]
fn bundle_presence_is_unknown_revision_and_glob_destinations_are_not_expanded() {
    // Given an app bundle at a literal path, plus a glob that could match it.
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("Tool.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(bundle.join("Contents/Info.plist"), "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>CFBundleShortVersionString</key><string>1.0</string></dict></plist>").unwrap();
    let literal = TrackedApp {
        kind: AppKind::App,
        ..configured(&bundle)
    };
    let glob = TrackedApp {
        kind: AppKind::App,
        ..configured(&root.path().join("*.app"))
    };
    // When observing these destinations.
    let view = AppView::new(literal, AppState::default(), false);
    let glob_presence = observe(&glob, None).unwrap();
    // Then only literal presence is used, without any revision or command execution.
    assert_eq!(view.status, AppStatus::Unknown);
    assert!(view.installed.is_none());
    assert_eq!(glob_presence, Presence::Unknown);
}

#[test]
fn absent_path_evidence_preserves_legacy_baselines() {
    // Given a manual baseline with neither recorded nor configured destination.
    let installed = InstalledRef {
        id: "abc1234".into(),
        sha: Some("abc1234".into()),
        version: None,
        installed_at: Utc::now(),
        path: None,
    };
    // When selecting a baseline.
    let result = retained(&cli_app("tool"), Some(installed.clone())).unwrap();
    // Then lack of path evidence does not falsely prove an uninstall.
    assert_eq!(result, Some(installed));
}

#[test]
fn non_executable_payload_invalidates_a_recorded_revision() {
    // Given a previously installed executable whose executable bit was removed.
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tool");
    std::fs::write(&path, "#!/bin/sh\nexit 99\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let app = configured(&path);
    let installed = InstalledRef {
        id: "a".repeat(40),
        sha: Some("a".repeat(40)),
        version: None,
        installed_at: Utc::now(),
        path: Some(path.display().to_string()),
    };
    // When reading the public view.
    let view = AppView::new(
        app,
        AppState {
            installed: Some(installed),
            ..Default::default()
        },
        false,
    );
    // Then the payload is classified as invalid and cannot claim that SHA is installed.
    assert_eq!(view.status, AppStatus::Unknown);
    assert!(view.installed.is_none());
    assert_eq!(view.installation.unwrap().presence, Presence::Invalid);
}

#[test]
fn destination_change_does_not_assign_the_previous_files_revision_to_another_file() {
    // Given two executable files at different paths and an old baseline for the first.
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("old");
    let new = root.path().join("new");
    for path in [&old, &new] {
        std::fs::write(path, "#!/bin/sh\nexit 99\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let installed = InstalledRef {
        id: "a".repeat(40),
        sha: Some("a".repeat(40)),
        version: None,
        installed_at: Utc::now(),
        path: Some(old.display().to_string()),
    };
    // When the configured destination points at the other file.
    let baseline = retained(&configured(&new), Some(installed)).unwrap();
    // Then planning cannot skip installation using the old path's revision.
    assert!(baseline.is_none());
}
