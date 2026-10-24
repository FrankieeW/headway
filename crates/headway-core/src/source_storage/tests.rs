use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{Install, SourceInstall};
use crate::events::{EventSink, LogStream, UpdateEvent};
use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::testing::{GitFixture, cli_app, with_github};
use crate::token::MemoryStore;
use crate::{CancellationToken, NullSink, Target, TrackedApp};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[path = "legacy_tests.rs"]
mod legacy;
#[path = "lifecycle_tests.rs"]
mod lifecycle;
#[path = "safety_tests.rs"]
mod safety;

const BUILD: &str = "printf '%s' \"$PWD\" > \"$TEST_WORKDIR\"; if [ -f target/cache ]; then echo warm; else echo cold; fi > \"$TEST_CACHE\"; mkdir -p target; echo retained > target/cache";

struct Fixture {
    root: tempfile::TempDir,
    _repo: GitFixture,
    _server: MockServer,
    hw: Headway,
    app: TrackedApp,
    sha: String,
    base: PathBuf,
    workdir: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    async fn new(keep: bool, custom: bool, build: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = GitFixture::new();
        let sha = repo.commit("payload", "installed-payload", "payload");
        let base = root.path().join("custom sources");
        let workdir = root.path().join("last-workdir");
        let cache = root.path().join("last-cache");
        let server = MockServer::start().await;
        Mock::given(path(format!("/repos/owner/tool/commits/{sha}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sha": sha, "commit": {"message": "payload"}
            })))
            .mount(&server)
            .await;
        let app = with_github(cli_app("tool"), |mut github| {
            github.git_url = Some(repo.url());
            github.install = Install::Source(SourceInstall {
                build: build.into(),
                artifact: Some("payload".into()),
                dest: Some(root.path().join("bin/tool").display().to_string()),
                storage: SourceStorage {
                    keep,
                    root: custom.then(|| base.display().to_string()),
                },
                ..SourceInstall::default()
            });
            github
        });
        let hw = Headway::builder()
            .paths(Paths::under(root.path()))
            .api(server.uri())
            .secrets(Arc::new(MemoryStore::default()))
            .env(
                ShellEnv::current()
                    .with_var("TEST_WORKDIR", workdir.display().to_string())
                    .with_var("TEST_CACHE", cache.display().to_string())
                    .with_var("GIT_CONFIG_GLOBAL", "/dev/null")
                    .with_var("GIT_CONFIG_NOSYSTEM", "1"),
            )
            .build()
            .unwrap();
        hw.add(app.clone()).unwrap();
        Self {
            root,
            _repo: repo,
            _server: server,
            hw,
            app,
            sha,
            base,
            workdir,
            cache,
        }
    }

    async fn update(&self) -> crate::Result<crate::UpdateOutcome> {
        self.hw
            .update(
                "tool",
                &Target::Ref(self.sha.clone()),
                &NullSink,
                &CancellationToken::new(),
            )
            .await
    }

    fn checkout(&self) -> PathBuf {
        PathBuf::from(fs::read_to_string(&self.workdir).unwrap())
    }

    fn with_storage(&self, storage: SourceStorage) -> TrackedApp {
        with_github(self.app.clone(), |mut github| {
            let Install::Source(ref mut source) = github.install else {
                unreachable!()
            };
            source.storage = storage;
            github
        })
    }
}

#[tokio::test]
async fn retained_custom_checkout_preserves_ignored_build_cache() {
    // Given a source build rooted in a user-selected directory.
    let f = Fixture::new(true, true, BUILD).await;
    let initial = f.hw.source_storage_status("tool").unwrap();
    assert!(!initial.exists);
    // When repeated builds use the managed retained checkout.
    f.update().await.unwrap();
    let first = f.checkout();
    assert_eq!(fs::read_to_string(&f.cache).unwrap().trim(), "cold");
    f.update().await.unwrap();
    // Then Git and ignored build output survive at exactly the selected child.
    assert_eq!(f.checkout(), first);
    assert_eq!(first, f.base.canonicalize().unwrap().join("tool"));
    assert_eq!(fs::read_to_string(&f.cache).unwrap().trim(), "warm");
    let status = f.hw.source_storage_status("tool").unwrap();
    assert!(status.keep && status.exists && status.custom_root);
    println!(
        "source_retained_status={}",
        serde_json::to_string(&status).unwrap()
    );
}

#[tokio::test]
async fn temporary_custom_checkout_is_fresh_and_removed_after_success() {
    // Given keep=false with a custom base containing unrelated user data.
    let f = Fixture::new(false, true, BUILD).await;
    fs::create_dir_all(&f.base).unwrap();
    fs::write(f.base.join("user-file"), "keep").unwrap();
    // When two successful source builds run.
    f.update().await.unwrap();
    let first = f.checkout();
    assert!(!first.exists());
    f.update().await.unwrap();
    let second = f.checkout();
    // Then both checkouts were fresh, selected-base children, and cleaned up.
    assert_ne!(first, second);
    assert!(second.starts_with(f.base.canonicalize().unwrap()));
    assert!(!second.exists());
    assert_eq!(fs::read_to_string(&f.cache).unwrap().trim(), "cold");
    assert_eq!(fs::read(f.base.join("user-file")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(&f.base).unwrap().count(), 1);
    assert!(!f.hw.source_storage_status("tool").unwrap().exists);
    println!(
        "source_temporary_observation={}",
        serde_json::json!({"first":first,"second":second,"removed":true,"cache":"cold"})
    );
}

#[tokio::test]
async fn temporary_checkout_is_removed_after_build_error() {
    // Given a failing build that records its real checkout path.
    let f = Fixture::new(
        false,
        true,
        "printf '%s' \"$PWD\" > \"$TEST_WORKDIR\"; exit 19",
    )
    .await;
    // When the build exits unsuccessfully.
    let error = f.update().await.unwrap_err();
    // Then the temporary clone is removed and failure state is truthful.
    assert!(matches!(
        error,
        Error::CommandFailed {
            status: Some(19),
            ..
        }
    ));
    assert!(!f.checkout().exists());
    assert_eq!(fs::read_dir(&f.base).unwrap().count(), 0);
    assert!(
        f.hw.store()
            .state()
            .unwrap()
            .app("tool")
            .installed
            .is_none()
    );
    assert!(!f.hw.history("tool").unwrap()[0].ok);
}

struct CancelBuild(CancellationToken);
impl EventSink for CancelBuild {
    fn emit(&self, event: UpdateEvent) {
        if matches!(event, UpdateEvent::Log { stream: LogStream::Stdout, line, .. } if line == "build-started")
        {
            self.0.cancel();
        }
    }
}

#[tokio::test]
async fn temporary_checkout_is_removed_after_cancelled_build() {
    // Given a real shell build that announces readiness before waiting.
    let f = Fixture::new(
        false,
        true,
        "printf '%s' \"$PWD\" > \"$TEST_WORKDIR\"; echo build-started; sleep 30",
    )
    .await;
    let cancel = CancellationToken::new();
    // When the emitted build event cancels the update.
    let result =
        f.hw.update(
            "tool",
            &Target::Ref(f.sha.clone()),
            &CancelBuild(cancel.clone()),
            &cancel,
        )
        .await;
    // Then the process is terminated and the temporary source is removed.
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!f.checkout().exists());
    assert_eq!(fs::read_dir(&f.base).unwrap().count(), 0);
}

#[tokio::test]
async fn default_retention_adopts_only_the_existing_expected_cache_clone() {
    // Given the pre-storage-version default Git cache with ignored compiler output.
    let f = Fixture::new(true, false, BUILD).await;
    let env = f.hw.env().await;
    let dir = f.hw.paths().source_dir("tool");
    let cancel = CancellationToken::new();
    crate::git::Checkout {
        dir: &dir,
        env: &env,
        cancel: &cancel,
    }
    .sync(
        f.app.github().unwrap().git_url.as_deref().unwrap(),
        &mut |_, _| {},
    )
    .await
    .unwrap();
    fs::create_dir_all(dir.join("target")).unwrap();
    fs::write(dir.join("target/cache"), "old-cache").unwrap();
    // When the new default configuration builds the same source.
    f.update().await.unwrap();
    // Then the exact legacy location and ignored output are retained.
    assert_eq!(f.checkout(), dir.canonicalize().unwrap());
    assert_eq!(fs::read_to_string(f.cache).unwrap().trim(), "warm");
    assert!(f.hw.source_storage_status("tool").unwrap().keep);
}
