use super::*;

async fn uncommitted(f: &Fixture, id: &str, bytes: &str) -> InstalledRef {
    fs::write(&f.src, bytes).unwrap();
    let candidate = InstalledRef {
        id: id.into(),
        sha: Some(id.into()),
        version: Some(id.into()),
        installed_at: Utc::now(),
        path: Some(f.dest.display().to_string()),
    };
    let env = ShellEnv::current();
    let cancel = CancellationToken::new();
    let log = RunLog::create(&f.hw.paths().app_log_dir("tool"), "tool", &NullSink).unwrap();
    Replacement {
        app: &f.app,
        paths: f.hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&f.src, Some(&candidate), &f.dest)
    .await
    .unwrap();
    candidate
}

#[tokio::test]
async fn crash_before_state_save_rolls_back_bytes_and_mark_style_reference() {
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.hw.store()
        .update_state(|state| {
            Ok((
                state.with_app("tool", |mut app| {
                    app.installed.as_mut().unwrap().path = None;
                    app
                }),
                (),
            ))
        })
        .unwrap();
    let previous = f.installed();
    uncommitted(&f, "new", "new bytes").await;
    assert!(super::pending(f.hw.paths(), &f.app));
    // Recovery intent is not disposable cache data.
    fs::remove_dir_all(&f.hw.paths().cache_dir).unwrap();
    super::reconcile(f.hw.paths(), &f.app).unwrap();
    super::reconcile(f.hw.paths(), &f.app).unwrap();
    assert_eq!(fs::read(&f.dest).unwrap(), b"old");
    assert_eq!(f.installed(), previous);
    assert!(!super::pending(f.hw.paths(), &f.app));
}

#[tokio::test]
async fn same_ref_reinstall_does_not_mistake_old_state_for_a_commit() {
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    uncommitted(&f, "old", "different uncommitted bytes").await;
    super::reconcile(f.hw.paths(), &f.app).unwrap();
    assert_eq!(fs::read(&f.dest).unwrap(), b"old");
    assert_eq!(f.installed().unwrap().id, "old");
}

#[tokio::test]
async fn crash_after_state_save_finalizes_verified_new_bytes() {
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    let candidate = uncommitted(&f, "new", "new bytes").await;
    f.hw.store()
        .update_state(|state| {
            Ok((
                state.with_app("tool", |app| app.installed_now(candidate)),
                (),
            ))
        })
        .unwrap();
    super::reconcile(f.hw.paths(), &f.app).unwrap();
    super::reconcile(f.hw.paths(), &f.app).unwrap();
    assert_eq!(fs::read(&f.dest).unwrap(), b"new bytes");
    assert_eq!(f.installed().unwrap().id, "new");
    assert!(!super::pending(f.hw.paths(), &f.app));
}
