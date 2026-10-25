use super::*;
use crate::testing::{FakeTools, cli_app, with_github};

fn fixture(env: ShellEnv) -> (tempfile::TempDir, Headway) {
    let root = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .env(env)
        .build()
        .unwrap();
    (root, hw)
}

fn at_destination(id: &str, dest: &str) -> TrackedApp {
    with_github(cli_app(id), |mut github| {
        let crate::Install::Source(source) = &mut github.install else {
            unreachable!()
        };
        source.dest = Some(dest.into());
        github
    })
}

#[test]
fn validation_is_read_only_and_matches_add_replace_destination_checks() {
    let (root, hw) = fixture(ShellEnv::from_vars([]));
    let app = with_github(cli_app("tool"), |github| GithubSource {
        repo: "https://github.com/owner/tool".into(),
        ..github
    });
    assert_eq!(
        hw.validate_add(app).unwrap().repo_id().unwrap().full_name(),
        "owner/tool"
    );
    assert!(!hw.paths().config_dir.exists());
    let dest = root.path().join("bin/shared").display().to_string();
    hw.add(at_destination("a", &dest)).unwrap();
    let config_before = std::fs::read(hw.paths().config_file()).unwrap();
    let state_before = std::fs::read(hw.paths().state_file()).ok();
    assert!(hw.validate_add(at_destination("b", &dest)).is_err());
    assert!(hw.add(at_destination("b", &dest)).is_err());
    hw.add(cli_app("b")).unwrap();
    assert!(hw.validate_replace(at_destination("b", &dest)).is_err());
    assert!(hw.replace(at_destination("b", &dest)).is_err());
    // Validation itself did not write (the intervening successful b registration did).
    assert_ne!(
        std::fs::read(hw.paths().config_file()).unwrap(),
        config_before
    );
    assert_eq!(std::fs::read(hw.paths().state_file()).ok(), state_before);
}

#[test]
fn new_registration_and_import_never_inherit_orphan_provenance() {
    for import in [false, true] {
        let (_root, hw) = fixture(ShellEnv::from_vars([]));
        hw.store
            .update_state(|state| {
                Ok((
                    state.with_app("tool", |state| AppState {
                        installed: Some(InstalledRef {
                            id: "foreign".into(),
                            sha: Some("foreign".into()),
                            version: None,
                            path: None,
                            installed_at: Utc::now(),
                        }),
                        last_error: Some(LastError::from_error(&Error::Cancelled, "update")),
                        auto_update_paused: true,
                        ..state
                    }),
                    (),
                ))
            })
            .unwrap();
        if import {
            hw.import_apps(vec![cli_app("tool")]).unwrap();
        } else {
            hw.add(cli_app("tool")).unwrap();
        }
        assert_eq!(hw.store.state().unwrap().app("tool"), AppState::default());
    }
}

#[test]
fn settings_validate_hand_edits_and_round_trip_language() {
    let (_root, hw) = fixture(ShellEnv::from_vars([]));
    crate::store::write_atomic(
        &hw.paths().config_file(),
        b"[settings]\nbin_dir = 'relative'\n",
    )
    .unwrap();
    assert_eq!(hw.settings().unwrap_err().code(), "invalid_config");
    crate::store::write_atomic(&hw.paths().config_file(), b"[settings]\nlanguage = 'zh'\n")
        .unwrap();
    let settings = hw.settings().unwrap();
    assert_eq!(settings.language, "zh");
    assert_eq!(hw.save_settings(&settings).unwrap(), settings);
    assert_eq!(hw.settings().unwrap(), settings);
    assert!(hw.set_setting("language", "fr").is_err());
    assert!(hw.set_setting("bin_dir", "~somebody/bin").is_err());
    assert!(hw.set_setting("check_interval_minutes", "525601").is_err());
}

#[tokio::test]
async fn plans_observe_detected_sources_without_writes_or_update_lock() {
    let tools = FakeTools::new()
        .tool("npm", r#"case "$1" in view) echo '{"versions":["1.0.0","2.0.0"],"dist-tags":{"latest":"2.0.0"}}';; ls) echo '{"dependencies":{"tool":{"version":"1.0.0"}}}';; *) exit 99;; esac"#)
        .tool("brew", r#"echo '{"formulae":[{"versions":{"stable":"2.0.0"},"installed":[{"version":"1.0.0"}],"outdated":true}]}'"#);
    for source in [
        Source::Npm(crate::NpmSource {
            package: "tool".into(),
            dist_tag: "latest".into(),
        }),
        Source::Brew(crate::BrewSource {
            package: "tool".into(),
            cask: false,
            install_path: None,
        }),
        Source::Script(crate::ScriptSource {
            update_command: "exit 99".into(),
            version_command: Some("echo 1.0.0".into()),
            latest_command: Some("echo 2.0.0".into()),
        }),
    ] {
        let (_root, hw) = fixture(tools.env());
        hw.add(TrackedApp {
            source,
            ..cli_app("tool")
        })
        .unwrap();
        hw.store.update_state(|state| Ok((state, ()))).unwrap();
        let before = std::fs::read(hw.paths().state_file()).unwrap();
        let backup_before =
            std::fs::read(hw.paths().state_file().with_extension("json.bak")).unwrap();
        let _lock = AppLock::acquire(&hw.paths().app_lock("tool"), "tool").unwrap();
        let plan = hw.plan("tool", &Target::Latest).await.unwrap();
        assert_eq!(plan.from.unwrap().id, "1.0.0");
        assert_eq!(plan.to.id, "2.0.0");
        assert_eq!(std::fs::read(hw.paths().state_file()).unwrap(), before);
        assert_eq!(
            std::fs::read(hw.paths().state_file().with_extension("json.bak")).unwrap(),
            backup_before
        );
    }
}

#[tokio::test]
async fn duplicate_check_ids_run_once_and_summary_preserves_both_contracts() {
    let (_root, hw) = fixture(ShellEnv::from_vars([]));
    hw.add(TrackedApp {
        source: Source::Script(crate::ScriptSource {
            update_command: "true".into(),
            version_command: Some("echo 1.0.0".into()),
            latest_command: Some("echo 2.0.0".into()),
        }),
        ..cli_app("tool")
    })
    .unwrap();
    let results = hw
        .check_many(&["tool".into(), "tool".into(), "missing".into()])
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    let summary = serde_json::to_value(CheckSummary::from_results(&results)).unwrap();
    assert_eq!(summary["checked"], 2);
    assert_eq!(summary["with_updates"], 1);
    assert_eq!(summary["results"][0]["latest"]["id"], "2.0.0");
    assert_eq!(summary["results"][1]["error"]["exit_code"], 3);
    assert_eq!(summary["failed"][0]["id"], "missing");
    assert_eq!(
        summary["failed"][0]["message"],
        results[1].1.as_ref().unwrap_err().to_string()
    );
}

#[tokio::test]
async fn cancelled_read_probes_return_without_persisting() {
    let (_root, hw) = fixture(ShellEnv::from_vars([]));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        hw.check_with_cancel("missing", &cancel)
            .await
            .unwrap_err()
            .code(),
        "cancelled"
    );
    assert_eq!(
        hw.plan_with_cancel("missing", &Target::Latest, &cancel)
            .await
            .unwrap_err()
            .code(),
        "cancelled"
    );
    assert!(!hw.paths().config_dir.exists());
}

#[test]
fn literal_destination_metacharacters_are_observed() {
    use std::os::unix::fs::PermissionsExt as _;
    let (root, _hw) = fixture(ShellEnv::from_vars([]));
    let dest = root.path().join("tool [beta] $1");
    std::fs::write(&dest, b"payload").unwrap();
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
    let app = at_destination("tool", dest.to_str().unwrap());
    assert_eq!(
        super::presence::observe(&app, None).unwrap(),
        Presence::Present
    );
}

#[test]
fn failed_state_write_never_exposes_a_new_source_with_foreign_provenance() {
    let (_root, hw) = fixture(ShellEnv::from_vars([]));
    hw.add(cli_app("tool")).unwrap();
    hw.store
        .update_state(|state| {
            Ok((
                state.with_app("tool", |state| {
                    state.installed_now(InstalledRef {
                        id: "a".repeat(40),
                        sha: Some("a".repeat(40)),
                        version: None,
                        installed_at: Utc::now(),
                        path: None,
                    })
                }),
                (),
            ))
        })
        .unwrap();
    let original = hw.store.config().unwrap();
    let original_state = hw.store.state().unwrap();
    let changed = with_github(original.apps[0].clone(), |github| GithubSource {
        repo: "another/tool".into(),
        ..github
    });
    // Force the backup persistence step to fail before state/config replacement.
    let backup = hw.paths().state_file().with_extension("json.bak");
    std::fs::remove_file(&backup).unwrap();
    std::fs::create_dir(&backup).unwrap();
    assert_eq!(hw.replace(changed.clone()).unwrap_err().code(), "io_error");
    assert_eq!(hw.store.config().unwrap(), original);
    assert_eq!(hw.store.state().unwrap(), original_state);
    std::fs::remove_dir(&backup).unwrap();
    let view = hw.replace(changed).unwrap();
    assert!(view.installed.is_none());
    let state = hw.store.state().unwrap().app("tool");
    assert_eq!(state.retained_installation.unwrap().repo, "owner/tool");
    assert!(state.installed.is_none());
}

#[test]
fn release_hex_and_date_prefixes_are_never_already_installed_targets() {
    for track in [
        Track::Release {
            prerelease: false,
            tag_filter: None,
        },
        Track::Tag {
            prerelease: false,
            tag_filter: None,
        },
    ] {
        let app = with_github(cli_app("tool"), |github| GithubSource { track, ..github });
        assert!(!target_matches(&app, "2024010", "20240101"));
        assert!(!target_matches(&app, "deadbee", "deadbeef"));
        assert!(target_matches(&app, "20240101", "20240101"));
    }
    assert!(target_matches(
        &cli_app("tool"),
        "abcdef1",
        "abcdef1234567890123456789012345678901234"
    ));
}

#[test]
fn views_and_mutations_honor_an_old_desktops_cache_only_lock() {
    let (_root, hw) = fixture(ShellEnv::from_vars([]));
    hw.add(cli_app("tool")).unwrap();
    let old = AppLock::acquire(&hw.paths().legacy_app_lock("tool"), "tool").unwrap();
    assert!(hw.get("tool").unwrap().busy);
    assert!(matches!(hw.remove("tool", false), Err(Error::Busy(_))));
    let changed = with_github(cli_app("tool"), |github| GithubSource {
        repo: "another/tool".into(),
        ..github
    });
    assert!(matches!(hw.replace(changed), Err(Error::Busy(_))));
    drop(old);
    assert!(!hw.get("tool").unwrap().busy);
}
