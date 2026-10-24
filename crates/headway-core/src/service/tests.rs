use super::*;
use crate::testing::{cli_app, with_github};
use crate::token::MemoryStore;

fn headway() -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .secrets(Arc::new(MemoryStore::default()))
        .env(ShellEnv::current())
        .api("http://127.0.0.1:9")
        .build()
        .unwrap();
    (dir, hw)
}

#[test]
fn add_get_replace_remove() {
    let (_dir, hw) = headway();
    let view = hw.add(cli_app("rg")).unwrap();
    assert_eq!(view.status, AppStatus::Unchecked);
    assert!(!view.busy);
    assert!(matches!(hw.add(cli_app("rg")), Err(Error::AppExists(_))));

    let edited = TrackedApp {
        name: "ripgrep".into(),
        ..cli_app("rg")
    };
    assert_eq!(hw.replace(edited).unwrap().app.name, "ripgrep");
    assert!(matches!(
        hw.replace(cli_app("nope")),
        Err(Error::AppNotFound(_))
    ));

    assert_eq!(hw.list().unwrap().len(), 1);
    hw.remove("rg", true).unwrap();
    assert!(hw.list().unwrap().is_empty());
    assert!(matches!(hw.remove("rg", false), Err(Error::AppNotFound(_))));
}

#[test]
fn changing_the_track_mode_forgets_the_installed_ref() {
    let (_dir, hw) = headway();
    hw.add(cli_app("rg")).unwrap();
    hw.store()
        .update_state(|state| {
            Ok((
                state.with_app("rg", |app| AppState {
                    installed: Some(InstalledRef {
                        id: "abc1234".into(),
                        sha: Some("abc1234".into()),
                        version: None,
                        installed_at: Utc::now(),
                        path: None,
                    }),
                    ..app
                }),
                (),
            ))
        })
        .unwrap();
    let branch_only = with_github(cli_app("rg"), |github| GithubSource {
        track: Track::Commit {
            branch: Some("dev".into()),
        },
        ..github
    });
    assert!(hw.replace(branch_only).unwrap().installed.is_some());
    let tags = with_github(cli_app("rg"), |github| GithubSource {
        track: Track::Tag {
            prerelease: false,
            tag_filter: None,
        },
        ..github
    });
    assert!(hw.replace(tags).unwrap().installed.is_none());
}

#[test]
fn busy_apps_cannot_be_removed() {
    let (_dir, hw) = headway();
    hw.add(cli_app("rg")).unwrap();
    let _held = AppLock::acquire(&hw.paths().app_lock("rg"), "rg").unwrap();
    assert_eq!(hw.get("rg").unwrap().status, AppStatus::Busy);
    assert!(matches!(hw.remove("rg", false), Err(Error::Busy(_))));
}

#[test]
fn settings_are_validated() {
    let (_dir, hw) = headway();
    let saved = hw
        .save_settings(&Settings {
            check_interval_minutes: 5,
            ..Settings::default()
        })
        .unwrap();
    assert_eq!(saved.check_interval_minutes, 5);
    assert!(
        hw.save_settings(&Settings {
            bin_dir: "relative".into(),
            ..Settings::default()
        })
        .is_err()
    );
    assert!(!hw.set_setting("notify", "false").unwrap().notify);
    assert!(!hw.settings().unwrap().notify);
}

#[test]
fn directory_destinations_are_rejected() {
    let (dir, hw) = headway();
    let app = with_github(cli_app("rg"), |github| GithubSource {
        install: crate::config::Install::Source(crate::config::SourceInstall {
            build: "make".into(),
            artifact: Some("tool".into()),
            dest: Some(dir.path().display().to_string()),
            ..crate::config::SourceInstall::default()
        }),
        ..github
    });
    let err = hw.add(app).unwrap_err();
    assert!(matches!(&err, Error::InvalidInput(msg) if msg.contains("existing directory")));
}

#[test]
fn repository_urls_are_stored_as_owner_name() {
    let (_dir, hw) = headway();
    let view = hw
        .add(with_github(cli_app("rg"), |github| GithubSource {
            repo: "https://github.com/owner/rg.git".into(),
            ..github
        }))
        .unwrap();
    assert_eq!(view.app.github().unwrap().repo, "owner/rg");
}

#[test]
fn unknown_apps_are_reported() {
    let (_dir, hw) = headway();
    assert!(matches!(hw.get("nope"), Err(Error::AppNotFound(_))));
    assert!(matches!(hw.history("nope"), Err(Error::AppNotFound(_))));
    assert!(matches!(hw.last_log("nope"), Err(Error::AppNotFound(_))));
}
