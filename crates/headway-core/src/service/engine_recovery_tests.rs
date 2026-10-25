//! Public service behavior across a crash after payload swap, before state commit.
use super::*;
use crate::testing::{cli_app, with_github};
use crate::token::MemoryStore;
use std::os::unix::fs::PermissionsExt as _;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

struct Interrupted {
    _root: tempfile::TempDir,
    _server: MockServer,
    hw: Headway,
    app: TrackedApp,
    dest: std::path::PathBuf,
    journal: std::path::PathBuf,
}

async fn interrupted() -> Interrupted {
    let root = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(path(format!(
        "/repos/owner/tool/commits/{}",
        "b".repeat(40)
    )))
    .respond_with(
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"sha":"b".repeat(40),"commit":{"message":"new"}})),
    )
    .mount(&server)
    .await;
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .api(server.uri())
        .secrets(Arc::new(MemoryStore::with_token("fixture-token")))
        .env(ShellEnv::from_vars([]))
        .build()
        .unwrap();
    let dest = root.path().join("bin/tool");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    executable(&dest, b"old");
    let app = with_github(cli_app("tool"), |mut github| {
        if let crate::Install::Source(source) = &mut github.install {
            source.dest = Some(dest.display().to_string());
        }
        github
    });
    hw.add(app.clone()).unwrap();
    let previous = InstalledRef {
        id: "a".repeat(40),
        sha: Some("a".repeat(40)),
        version: None,
        path: Some(dest.display().to_string()),
        installed_at: Utc::now(),
    };
    hw.store
        .update_state(|state| {
            Ok((
                state.with_app("tool", |state| state.installed_now(previous)),
                (),
            ))
        })
        .unwrap();
    let candidate = InstalledRef {
        id: "b".repeat(40),
        sha: Some("b".repeat(40)),
        version: None,
        path: Some(dest.display().to_string()),
        installed_at: Utc::now(),
    };
    let src = root.path().join("new");
    executable(&src, b"new");
    let lock = AppLock::acquire(&hw.paths().app_lock("tool"), "tool").unwrap();
    let log = RunLog::create(&hw.paths().app_log_dir("tool"), "tool", &crate::NullSink).unwrap();
    let env = hw.env().await;
    let cancel = CancellationToken::new();
    crate::recovery::Replacement {
        app: &app,
        paths: hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&src, Some(&candidate), &dest)
    .await
    .unwrap();
    // Simulated crash: no state commit and no recovery::commit call.
    drop(lock);
    let journal = hw.paths().config_dir.join("transactions/tool.json");
    assert!(journal.exists());
    Interrupted {
        _root: root,
        _server: server,
        hw,
        app,
        dest,
        journal,
    }
}

fn executable(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn interrupted_views_and_plans_do_not_claim_or_mutate_an_uncertain_baseline() {
    let f = interrupted().await;
    let state = std::fs::read(f.hw.paths().state_file()).unwrap();
    let journal = std::fs::read(&f.journal).unwrap();
    let view = f.hw.get("tool").unwrap();
    assert!(view.installed.is_none());
    assert_eq!(view.last_error.unwrap().code, "recovery_pending");
    let plan =
        f.hw.plan("tool", &Target::Ref("b".repeat(40)))
            .await
            .unwrap();
    assert!(plan.from.is_none());
    assert!(!plan.up_to_date);
    assert!(plan.steps.iter().any(|step| step.contains("recovery")));
    assert_eq!(std::fs::read(f.hw.paths().state_file()).unwrap(), state);
    assert_eq!(std::fs::read(&f.journal).unwrap(), journal);
    assert_eq!(std::fs::read(&f.dest).unwrap(), b"new");
}

#[tokio::test]
async fn removal_reconciles_interrupted_payload_before_reusing_the_id() {
    for purge in [false, true] {
        let f = interrupted().await;
        f.hw.remove("tool", purge).unwrap();
        assert!(!f.journal.exists());
        assert_eq!(std::fs::read(&f.dest).unwrap(), b"old");
        let replacement = with_github(f.app, |github| GithubSource {
            repo: "another/tool".into(),
            ..github
        });
        let view = f.hw.add(replacement).unwrap();
        assert!(view.installed.is_none());
        assert!(view.last_error.is_none());
        assert_eq!(f.hw.store.state().unwrap().app("tool"), AppState::default());
    }
}

#[tokio::test]
async fn source_replacement_reconciles_the_old_identity_first() {
    let f = interrupted().await;
    let replacement = with_github(f.app, |github| GithubSource {
        repo: "another/tool".into(),
        ..github
    });
    assert!(f.hw.validate_replace(replacement.clone()).is_ok());
    let view = f.hw.replace(replacement).unwrap();
    assert!(!f.journal.exists());
    assert!(view.installed.is_none());
    assert_eq!(std::fs::read(f.dest).unwrap(), b"old");
    assert_eq!(
        f.hw.store
            .state()
            .unwrap()
            .app("tool")
            .retained_installation
            .unwrap()
            .repo,
        "owner/tool"
    );
}

#[tokio::test]
async fn unrecoverable_journal_refuses_removal_without_losing_evidence() {
    let f = interrupted().await;
    std::fs::write(&f.journal, b"corrupt journal").unwrap();
    assert!(f.hw.remove("tool", true).is_err());
    assert!(f.hw.store.config().unwrap().app("tool").is_ok());
    assert_eq!(std::fs::read(f.journal).unwrap(), b"corrupt journal");
    assert_eq!(std::fs::read(f.dest).unwrap(), b"new");
}

#[tokio::test]
async fn manually_removed_configuration_cannot_silently_reuse_an_interrupted_id() {
    let f = interrupted().await;
    f.hw.store
        .update_config(|mut config| {
            config.apps.clear();
            Ok((config, ()))
        })
        .unwrap();
    let replacement = with_github(f.app, |github| GithubSource {
        repo: "another/tool".into(),
        ..github
    });
    assert_eq!(
        f.hw.validate_add(replacement.clone()).unwrap_err().code(),
        "recovery_pending"
    );
    assert_eq!(
        f.hw.plan_import_apps(vec![replacement]).unwrap_err().code(),
        "recovery_pending"
    );
    assert!(f.journal.exists());
}
