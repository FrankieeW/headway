use super::*;
use crate::config::{Install, SourceInstall};
use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::testing::{cli_app, with_github};
use crate::update::RunLog;
use crate::{CancellationToken, Headway, NullSink};
use std::fs;
use std::path::PathBuf;

#[path = "identity_tests.rs"]
mod identity;
#[path = "identity_update_tests.rs"]
mod identity_updates;
#[path = "safety_tests.rs"]
mod safety;
#[path = "update_tests.rs"]
mod updates;

struct Fixture {
    dir: tempfile::TempDir,
    hw: Headway,
    app: crate::TrackedApp,
    dest: PathBuf,
    src: PathBuf,
}

impl Fixture {
    fn new(command: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("bin/tool");
        let src = dir.path().join("payload");
        let app = with_github(cli_app("tool"), |github| crate::GithubSource {
            install: Install::Source(SourceInstall {
                build: "false".into(),
                artifact: Some("payload".into()),
                dest: Some(dest.display().to_string()),
                verify_command: command.map(str::to_owned),
                ..SourceInstall::default()
            }),
            ..github
        });
        let hw = Headway::builder()
            .paths(Paths::under(dir.path()))
            .build()
            .unwrap();
        hw.add(app.clone()).unwrap();
        Self {
            dir,
            hw,
            app,
            dest,
            src,
        }
    }

    async fn place(&self, bytes: &str) -> crate::Result<()> {
        fs::write(&self.src, bytes).unwrap();
        let env = ShellEnv::current();
        let log = RunLog::create(&self.hw.paths().app_log_dir("tool"), "tool", &NullSink).unwrap();
        let cancel = CancellationToken::new();
        let candidate = InstalledRef {
            id: bytes.into(),
            sha: Some(bytes.into()),
            version: Some(bytes.into()),
            path: Some(self.dest.display().to_string()),
            installed_at: Utc::now(),
        };
        Replacement {
            app: &self.app,
            paths: self.hw.paths(),
            env: &env,
            log: &log,
            cancel: &cancel,
        }
        .place(&self.src, Some(&candidate), &self.dest)
        .await?;
        self.hw
            .store()
            .update_state(|state| {
                Ok((
                    state.with_app("tool", |app| app.installed_now(candidate)),
                    (),
                ))
            })
            .unwrap();
        Ok(())
    }

    fn installed(&self) -> Option<InstalledRef> {
        self.hw.store().state().unwrap().app("tool").installed
    }
}

#[tokio::test]
async fn failed_verifier_restores_exact_bytes_and_reference() {
    // Given a known installation and a verifier that rejects the next candidate.
    let f = Fixture::new(Some("test \"$(cat \"$HEADWAY_INSTALLED_PATH\")\" != bad"));
    f.place("old").await.unwrap();
    let old = f.installed();
    // When the replacement fails verification.
    assert!(f.place("bad").await.is_err());
    // Then the old bytes and reference survive without retaining a failed payload.
    assert_eq!(fs::read(&f.dest).unwrap(), b"old");
    assert_eq!(f.installed(), old);
    assert!(f.hw.list_backups("tool").unwrap().is_empty());
}

#[tokio::test]
async fn retains_two_prior_snapshots_and_restores_without_rebuild() {
    // Given four successful installations (the configured build deliberately fails).
    let f = Fixture::new(None);
    for value in ["one", "two", "three", "four"] {
        f.place(value).await.unwrap();
    }
    let backups = f.hw.list_backups("tool").unwrap();
    assert_eq!(backups.len(), 2);
    assert_eq!(
        backups
            .iter()
            .map(|b| b.installed.as_ref().unwrap().id.as_str())
            .collect::<Vec<_>>(),
        ["three", "two"]
    );
    let bytes = fs::read(&backups[1].artifact_path).unwrap();
    // When restoring directly from the cache.
    let result =
        f.hw.restore_backup("tool", &backups[1].id, &NullSink, &CancellationToken::new())
            .await
            .unwrap();
    // Then no build is required and restored state identifies the exact old payload.
    assert_eq!(fs::read(&f.dest).unwrap(), bytes);
    assert_eq!(result.installed.unwrap().id, "two");
    assert_eq!(f.installed().unwrap().id, "two");
    assert!(
        f.hw.store()
            .state()
            .unwrap()
            .app("tool")
            .last_check
            .is_none()
    );
    assert_eq!(f.hw.list_backups("tool").unwrap().len(), 2);
    assert!(f.hw.history("tool").unwrap().last().unwrap().ok);
}

#[tokio::test]
async fn failed_restore_keeps_cache_and_current_payload() {
    // Given two cached backups and a newly configured failing verifier.
    let f = Fixture::new(None);
    for value in ["one", "two", "three"] {
        f.place(value).await.unwrap();
    }
    let backups = f.hw.list_backups("tool").unwrap();
    let old = f.installed();
    let edited = with_github(f.app.clone(), |mut github| {
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.verify_command = Some("exit 7".into());
        github
    });
    f.hw.replace(edited).unwrap();
    // When restoring a valid cached payload fails verification.
    assert!(
        f.hw.restore_backup("tool", &backups[0].id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    // Then exact old bytes/ref and both cached payloads remain, with failed history.
    assert_eq!(fs::read(&f.dest).unwrap(), b"three");
    assert_eq!(f.installed(), old);
    assert_eq!(f.hw.list_backups("tool").unwrap().len(), 2);
    for backup in backups {
        assert!(PathBuf::from(backup.artifact_path).exists());
    }
    assert!(!f.hw.history("tool").unwrap().last().unwrap().ok);
}

#[tokio::test]
async fn unknown_existing_payload_restores_without_inventing_a_version() {
    // Given existing bytes without Headway version metadata.
    let f = Fixture::new(None);
    fs::create_dir_all(f.dest.parent().unwrap()).unwrap();
    fs::write(&f.dest, "external-old").unwrap();
    f.place("new").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    assert!(backup.installed.is_none());
    // When restoring the first captured payload.
    let result =
        f.hw.restore_backup("tool", &backup.id, &NullSink, &CancellationToken::new())
            .await
            .unwrap();
    // Then bytes return and version certainty is explicitly absent.
    assert_eq!(fs::read(&f.dest).unwrap(), b"external-old");
    assert!(result.installed.is_none());
    assert!(f.installed().is_none());
}

#[tokio::test]
async fn failed_first_install_removes_unverified_payload() {
    // Given no previous installation.
    let f = Fixture::new(Some("exit 1"));
    // When the first candidate fails verification.
    assert!(f.place("bad").await.is_err());
    // Then neither bytes nor a fabricated installed reference remain.
    assert!(!f.dest.exists());
    assert!(f.installed().is_none());
}

#[tokio::test]
async fn corrupted_payload_and_traversal_are_rejected() {
    // Given a retained payload that is modified on disk.
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    f.place("new").await.unwrap();
    let backup = f.hw.list_backups("tool").unwrap().remove(0);
    fs::write(&backup.artifact_path, "corrupt").unwrap();
    // When loading corrupt or traversal identifiers.
    for id in [backup.id.as_str(), "../tool", "/tmp", "..", ""] {
        assert!(
            f.hw.restore_backup("tool", id, &NullSink, &CancellationToken::new())
                .await
                .is_err()
        );
    }
    // Then the current destination is untouched.
    assert_eq!(fs::read(&f.dest).unwrap(), b"new");
    assert!(f.hw.list_backups("../tool").is_err());
}

#[test]
fn external_and_self_installing_providers_are_unsupported() {
    // Given each provider without a managed artifact.
    let f = Fixture::new(None);
    let sources = [
        crate::Source::Npm(crate::NpmSource {
            package: "tool".into(),
            dist_tag: "latest".into(),
        }),
        crate::Source::Brew(crate::BrewSource {
            install_path: None,
            package: "tool".into(),
            cask: false,
        }),
        crate::Source::Script(crate::ScriptSource {
            update_command: "true".into(),
            version_command: None,
            latest_command: None,
        }),
    ];
    // When requesting recovery support.
    for source in sources {
        f.hw.replace(crate::TrackedApp {
            source,
            ..f.app.clone()
        })
        .unwrap();
        assert!(matches!(
            f.hw.list_backups("tool"),
            Err(crate::Error::InvalidInput(_))
        ));
    }
    let app = with_github(f.app.clone(), |mut github| {
        github.install = Install::Source(SourceInstall {
            build: "true".into(),
            ..Default::default()
        });
        github
    });
    // Then a self-installing build is also rejected.
    f.hw.replace(app).unwrap();
    assert!(matches!(
        f.hw.list_backups("tool"),
        Err(crate::Error::InvalidInput(_))
    ));
}
