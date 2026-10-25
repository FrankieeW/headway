use super::*;
use crate::testing::cli_app;

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(Paths::under(dir.path()));
    (dir, store)
}

#[test]
fn state_schema_accepts_legacy_and_refuses_future_without_writes() {
    let (_dir, store) = store();
    let path = store.paths.state_file();
    assert_eq!(
        serde_json::from_str::<State>(r#"{"apps":{}}"#)
            .unwrap()
            .version,
        0
    );
    write_atomic(&path, br#"{"apps":{}}"#).unwrap();
    assert_eq!(store.state().unwrap().version, crate::state::STATE_VERSION);
    let future = br#"{"version":4294967295,"apps":{}}"#;
    write_atomic(&path, future).unwrap();
    assert_eq!(store.state().unwrap_err().code(), "invalid_config");
    assert!(store.update_state(|state| Ok((state, ()))).is_err());
    assert_eq!(fs::read(path).unwrap(), future);
    assert!(!store.paths.state_file().with_extension("json.bak").exists());
}

#[test]
fn corrupt_state_preserves_original_and_last_valid_backup() {
    let (_dir, store) = store();
    store
        .update_state(|state| Ok((state.with_app("old", |state| state), ())))
        .unwrap();
    store
        .update_state(|state| Ok((state.with_app("new", |state| state), ())))
        .unwrap();
    let backup = store.paths.state_file().with_extension("json.bak");
    let previous = fs::read(&backup).unwrap();
    let saved: State = serde_json::from_slice(&previous).unwrap();
    assert!(saved.apps.contains_key("old"));
    assert!(!saved.apps.contains_key("new"));
    let corrupt = b"{not valid state";
    write_atomic(&store.paths.state_file(), corrupt).unwrap();
    assert!(
        store
            .state()
            .unwrap_err()
            .to_string()
            .contains("state.json.bak")
    );
    assert!(store.update_state(|state| Ok((state, ()))).is_err());
    assert_eq!(fs::read(store.paths.state_file()).unwrap(), corrupt);
    assert_eq!(fs::read(backup).unwrap(), previous);
}

#[test]
fn atomic_files_are_private_and_failed_replace_cleans_temporary_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("state");
    write_atomic(&target, b"old").unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let directory = dir.path().join("existing-directory");
    fs::create_dir(&directory).unwrap();
    assert!(write_atomic(&directory, b"never replaces a directory").is_err());
    assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".headway-")
    }));
}

#[test]
fn content_fingerprint_detects_preserved_mtime_same_size_rewrites() {
    let (_dir, store) = store();
    let path = store.paths.config_file();
    write_atomic(&path, b"# one").unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let before = store.fingerprint();
    fs::write(&path, b"# two").unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_ne!(store.fingerprint(), before);
}

#[test]
fn concurrent_store_writers_merge_instead_of_losing_updates() {
    let (_dir, store) = store();
    let threads: Vec<_> = (0..8)
        .map(|index| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .update_config(|mut config| {
                        config.apps.push(cli_app(&format!("tool-{index}")));
                        Ok((config, ()))
                    })
                    .unwrap();
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(store.config().unwrap().apps.len(), 8);
}

#[test]
fn cross_process_store_worker() {
    let Some(root) = std::env::var_os("HEADWAY_STORE_TEST_ROOT") else {
        return;
    };
    let prefix = std::env::var("HEADWAY_STORE_TEST_PREFIX").unwrap();
    let store = Store::new(Paths::under(root));
    for index in 0..5 {
        let id = format!("{prefix}-{index}");
        store
            .update_config(|mut config| {
                config.apps.push(cli_app(&id));
                Ok((config, ()))
            })
            .unwrap();
        store
            .update_state(|state| Ok((state.with_app(&id, |state| state), ())))
            .unwrap();
    }
}

#[test]
fn two_processes_merge_config_and_state_updates() {
    let (dir, store) = store();
    let executable = std::env::current_exe().unwrap();
    let mut children: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|prefix| {
            std::process::Command::new(&executable)
                .args(["--exact", "store::regressions::cross_process_store_worker"])
                .env("HEADWAY_STORE_TEST_ROOT", dir.path())
                .env("HEADWAY_STORE_TEST_PREFIX", prefix)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(store.config().unwrap().apps.len(), 10);
    assert_eq!(store.state().unwrap().apps.len(), 10);
    assert!(
        serde_json::from_slice::<State>(
            &fs::read(store.paths.state_file().with_extension("json.bak")).unwrap()
        )
        .is_ok()
    );
}

#[test]
fn hung_store_peer_returns_bounded_store_busy() {
    let (_dir, store) = store();
    let held = store.lock().unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        store
            .update_config(|config| Ok((config, ())))
            .unwrap_err()
            .code(),
        "store_busy"
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    drop(held);
}

#[test]
fn failed_config_commit_cannot_attach_old_state_to_the_proposed_source() {
    let (_dir, store) = store();
    store
        .update_config(|mut config| {
            config.apps.push(cli_app("tool"));
            Ok((config, ()))
        })
        .unwrap();
    store
        .update_state(|state| {
            Ok((
                state.with_app("tool", |state| crate::state::AppState {
                    last_error: Some(crate::state::LastError::from_error(
                        &Error::Cancelled,
                        "check",
                    )),
                    ..state
                }),
                (),
            ))
        })
        .unwrap();
    let result = store.register(|config, state| {
        // Failure injection after reads, before the configuration rename.
        fs::remove_file(store.paths.config_file()).unwrap();
        fs::create_dir(store.paths.config_file()).unwrap();
        Ok((config, state.without_app("tool"), ()))
    });
    assert!(result.is_err());
    assert!(!store.state().unwrap().apps.contains_key("tool"));
    let backup: State = serde_json::from_slice(
        &fs::read(store.paths.state_file().with_extension("json.bak")).unwrap(),
    )
    .unwrap();
    assert!(backup.apps.contains_key("tool"));
}

#[test]
fn stale_source_check_cannot_write_after_a_registration_change() {
    let (_dir, store) = store();
    let old = cli_app("tool");
    store
        .update_config(|mut config| {
            config.apps.push(old.clone());
            Ok((config, ()))
        })
        .unwrap();
    let changed = crate::testing::with_github(old.clone(), |mut github| {
        github.repo = "another/tool".into();
        github
    });
    store
        .update_config(|mut config| {
            config.apps[0] = changed;
            Ok((config, ()))
        })
        .unwrap();
    let error = store
        .update_state_for_source("tool", &old.source, |state| {
            Ok((state.with_app("tool", |state| state), ()))
        })
        .unwrap_err();
    assert_eq!(error.code(), "config_changed");
    assert_eq!(error.exit_code(), 1);
    assert!(!store.paths.state_file().exists());
    assert!(error.hint().unwrap().contains("re-run"));
}
