use super::*;

fn candidate(id: &str) -> Candidate {
    Candidate {
        id: id.into(),
        sha: None,
        title: format!("commit {id}"),
        body: None,
        author: None,
        date: None,
        url: None,
        prerelease: false,
    }
}

fn check(pending: &[&str], latest: &str) -> CheckResult {
    CheckResult {
        checked_at: Utc::now(),
        status: UpdateStatus::UpdateAvailable,
        branch: Some("main".into()),
        latest: Some(candidate(latest)),
        pending: pending.iter().map(|id| candidate(id)).collect(),
        pending_total: u32::try_from(pending.len()).unwrap(),
    }
}

#[test]
fn rebasing_on_the_latest_clears_pending() {
    let rebased = check(&["ccc", "bbb", "aaa"], "ccc").rebased_on("ccc");
    assert_eq!(rebased.status, UpdateStatus::UpToDate);
    assert!(rebased.pending.is_empty());
    assert_eq!(rebased.pending_total, 0);
}

#[test]
fn rebasing_in_the_middle_keeps_newer_candidates() {
    let rebased = check(&["ccc", "bbb", "aaa"], "ccc").rebased_on("bbb");
    assert_eq!(rebased.status, UpdateStatus::UpdateAvailable);
    assert_eq!(rebased.pending, vec![candidate("ccc")]);
    assert_eq!(rebased.pending_total, 1);
    assert_eq!(rebased.next().unwrap().id, "ccc");
}

#[test]
fn rebasing_outside_the_window_reports_the_latest() {
    let rebased = check(&[], "ccc").rebased_on("zzz");
    assert_eq!(rebased.status, UpdateStatus::UpdateAvailable);
    assert_eq!(rebased.pending, vec![candidate("ccc")]);
    assert_eq!(rebased.pending_total, 1);
}

#[test]
fn short_and_full_commit_shas_match() {
    assert!(ids_match("abcdef1234567890", "abcdef1"));
    assert!(ids_match("abcdef1", "abcdef1234567890"));
    assert!(!ids_match("abcdef1", "abcdef2"));
    assert!(!ids_match("v1.2", "v1.2.0"), "tags must match exactly");
    assert!(!ids_match("abc", "abcdef1"), "too short to be a sha prefix");
}

#[test]
fn history_is_capped() {
    let entry = |n: usize| HistoryEntry {
        at: Utc::now(),
        action: HistoryAction::Update,
        from: None,
        to: n.to_string(),
        version: None,
        ok: true,
        message: None,
        log_path: None,
        duration_ms: None,
    };
    let state = (0..HISTORY_LIMIT + 5).fold(AppState::default(), |state, n| state.record(entry(n)));
    assert_eq!(state.history.len(), HISTORY_LIMIT);
    assert_eq!(state.history[0].to, "5");
}

#[test]
fn with_app_returns_a_new_state() {
    let state = State::default().with_app("rg", |app| AppState {
        last_error: Some(LastError {
            code: "x".into(),
            message: "boom".into(),
            at: Utc::now(),
            during: "check".into(),
        }),
        ..app
    });
    assert!(state.app("rg").last_error.is_some());
    assert_eq!(state.app("missing"), AppState::default());
    assert!(state.without_app("rg").apps.is_empty());
}

#[test]
fn installed_now_clears_errors_and_rebases() {
    let app = AppState {
        last_check: Some(check(&["bbb", "aaa"], "bbb")),
        last_error: Some(LastError {
            code: "command_failed".into(),
            message: "boom".into(),
            at: Utc::now(),
            during: "update".into(),
        }),
        ..AppState::default()
    };
    let installed = InstalledRef {
        id: "bbb".into(),
        sha: Some("bbb".into()),
        version: None,
        installed_at: Utc::now(),
        path: None,
    };
    let next = app.installed_now(installed.clone());
    assert_eq!(next.installed, Some(installed));
    assert!(next.last_error.is_none());
    assert_eq!(next.last_check.unwrap().status, UpdateStatus::UpToDate);
}

#[test]
fn json_round_trip() {
    let state = State::default().with_app("rg", |app| AppState {
        last_check: Some(check(&["b"], "b")),
        ..app
    });
    let text = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<State>(&text).unwrap(), state);
}

#[test]
fn release_and_tag_checks_never_rebase_on_a_hex_or_numeric_prefix() {
    for (installed, latest) in [("2024010", "20240101"), ("deadbee", "deadbeef")] {
        let mut result = check(&[latest], latest);
        result.branch = None;
        let rebased = result.rebased_on(installed);
        assert_eq!(rebased.status, UpdateStatus::UpdateAvailable);
        assert_eq!(rebased.pending_total, 1);
    }
}
