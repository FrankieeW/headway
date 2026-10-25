//! Tests for `Headway::self_restart_pending`, the cheap desktop monitor gate.
#![allow(clippy::unwrap_used)] // isolated fixture setup must fail loudly

use super::*;

fn headway(dir: &std::path::Path) -> Headway {
    Headway::builder()
        .paths(crate::paths::Paths::under(dir))
        .build()
        .unwrap()
}

fn request(app_path: &str) -> RestartRequest {
    RestartRequest {
        installed: BuildInfo::current(),
        app_path: app_path.into(),
        dispatched: false,
        confirmed: false,
        restart_error: None,
    }
}

#[test]
fn legacy_restart_control_lock_still_guards_the_same_request_file() {
    let dir = tempfile::tempdir().unwrap();
    let hw = headway(dir.path());
    let old = crate::lock::AppLock::acquire(
        &hw.paths().config_dir.join("self-restart.lock"),
        "self-restart",
    )
    .unwrap();
    assert!(matches!(hw.confirm_self_restart(), Err(Error::Busy(id)) if id == "self-restart"));
    drop(old);
    assert!(!hw.confirm_self_restart().unwrap());
}

#[test]
fn no_request_is_not_pending() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!headway(dir.path()).self_restart_pending().unwrap());
}

#[test]
fn only_an_actionable_request_for_this_installation_is_pending() {
    let dir = tempfile::tempdir().unwrap();
    let hw = headway(dir.path());
    // The fixture app path "/" contains every real executable, standing in for this install.
    hw.save_restart(&request("/")).unwrap();
    assert!(hw.self_restart_pending().unwrap());
    // A request addressed to another installation is not actionable here.
    hw.save_restart(&request("/definitely-not-this-bundle.app"))
        .unwrap();
    assert!(!hw.self_restart_pending().unwrap());
}

#[test]
fn consumed_confirmed_and_failed_requests_are_not_pending() {
    let dir = tempfile::tempdir().unwrap();
    let hw = headway(dir.path());
    let mut dispatched = request("/");
    dispatched.dispatched = true;
    let mut confirmed = request("/");
    confirmed.confirmed = true;
    let mut failed = request("/");
    failed.restart_error = Some("failed".into());
    for pending in [dispatched, confirmed, failed] {
        hw.save_restart(&pending).unwrap();
        assert!(!hw.self_restart_pending().unwrap());
    }
}

#[test]
fn unparsable_request_is_not_pending_and_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let hw = headway(dir.path());
    let path = hw.restart_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{not json").unwrap();
    assert!(!hw.self_restart_pending().unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
}
