use crate::events::{LogStream, Stage, UpdateEvent};
use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn targets_parse() {
    assert_eq!(Target::parse("latest"), Target::Latest);
    assert_eq!(Target::parse(" next "), Target::Next);
    assert_eq!(Target::parse("v1.2.3"), Target::Ref("v1.2.3".into()));
    assert_eq!(
        serde_json::to_string(&Target::Ref("abc".into())).unwrap(),
        r#"{"ref":"abc"}"#
    );
    assert_eq!(
        serde_json::from_str::<Target>(r#""next""#).unwrap(),
        Target::Next
    );
}

#[test]
fn artifacts_resolve_with_globs() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_dir = dir.path().join("bundle/macos");
    fs::create_dir_all(bundle_dir.join("Demo.app")).unwrap();
    fs::write(dir.path().join("tool"), "").unwrap();
    assert_eq!(
        resolve_artifact(dir.path(), "tool").unwrap(),
        dir.path().join("tool")
    );
    assert_eq!(
        resolve_artifact(dir.path(), "bundle/macos/*.app").unwrap(),
        bundle_dir.join("Demo.app")
    );
    assert!(
        resolve_artifact(dir.path(), "missing")
            .unwrap_err()
            .to_string()
            .contains("does not exist")
    );
    assert!(resolve_artifact(dir.path(), "bundle/macos/*.zip").is_err());
}

#[test]
fn run_log_writes_file_and_events() {
    let dir = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink_events = Arc::clone(&events);
    let sink = move |event: UpdateEvent| sink_events.lock().unwrap().push(event);
    let log = RunLog::create(dir.path(), "rg", &sink).unwrap();
    log.stage(Stage::Build, "Running `make`");
    log.line(LogStream::Stderr, "warning: x");
    log.progress(1, Some(10));
    log.progress(2, Some(10)); // throttled
    log.progress(10, Some(10)); // final, always sent

    let text = fs::read_to_string(log.path()).unwrap();
    assert!(text.contains("[headway] Running `make`"));
    assert!(text.contains("[err] warning: x"));
    let events = events.lock().unwrap();
    let progress = events
        .iter()
        .filter(|e| matches!(e, UpdateEvent::Progress { .. }))
        .count();
    assert_eq!(progress, 2);
    assert!(matches!(
        &events[0],
        UpdateEvent::Stage {
            stage: Stage::Build,
            ..
        }
    ));
    assert_eq!(latest_log(dir.path()).as_deref(), Some(log.path()));
}

#[test]
fn old_logs_are_pruned() {
    let dir = tempfile::tempdir().unwrap();
    for n in 0..LOG_KEEP + 3 {
        fs::write(dir.path().join(format!("2026010{n:02}.log")), "").unwrap();
    }
    prune_logs(dir.path(), LOG_KEEP);
    assert_eq!(log_files(dir.path()).len(), LOG_KEEP);
    assert!(!dir.path().join("202601000.log").exists());
}

#[test]
fn formatting_helpers() {
    assert_eq!(short("0123456789abcdef0123"), "0123456");
    assert_eq!(short("v1.2.3"), "v1.2.3");
    assert_eq!(short("refs/tags/v1"), "refs/tags/v1");
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(5 * 1024 * 1024), "5120 KB");
    assert_eq!(human_bytes(50 * 1024 * 1024), "50 MB");
}
