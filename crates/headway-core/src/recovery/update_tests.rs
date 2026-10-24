use super::*;
use crate::Target;
use crate::events::{EventSink, LogStream, UpdateEvent};
use crate::testing::GitFixture;
use crate::token::MemoryStore;
use std::sync::Arc;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[tokio::test]
async fn real_update_failure_preserves_bytes_ref_cache_and_records_failure() {
    // Given a real source build and a GitHub API fixture for its exact commit.
    let f = Fixture::new(None);
    let repo = GitFixture::new();
    let sha = repo.commit("payload", "candidate", "candidate");
    let server = MockServer::start().await;
    Mock::given(path(format!("/repos/owner/tool/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha, "commit": {"message": "candidate"}
        })))
        .mount(&server)
        .await;
    let hw = Headway::builder()
        .paths(f.hw.paths().clone())
        .api(server.uri())
        .secrets(Arc::new(MemoryStore::default()))
        .env(ShellEnv::current())
        .build()
        .unwrap();
    f.place("oldest").await.unwrap();
    f.place("old").await.unwrap();
    let previous = f.installed();
    let app = with_github(f.app.clone(), |mut github| {
        github.git_url = Some(repo.url());
        github.install = Install::Source(SourceInstall {
            build: "test -f payload".into(), artifact: Some("payload".into()),
            dest: Some(f.dest.display().to_string()),
            verify_command: Some("test \"$HEADWAY_REF\" = \"$HEADWAY_SHA\" && test \"$(cat \"$HEADWAY_INSTALLED_PATH\")\" = candidate; exit 19".into()),
            ..Default::default()
        });
        github
    });
    hw.replace(app).unwrap();
    // When the real service update builds/copies successfully but fails verification.
    let error = hw
        .update(
            "tool",
            &Target::Ref(sha.clone()),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    // Then state/history/cache reflect failure without losing the usable installation.
    assert!(matches!(
        error,
        crate::Error::CommandFailed {
            status: Some(19),
            ..
        }
    ));
    assert_eq!(fs::read(&f.dest).unwrap(), b"old");
    assert_eq!(f.installed(), previous);
    assert_eq!(hw.list_backups("tool").unwrap().len(), 1);
    let history = hw.history("tool").unwrap();
    let entry = history.last().unwrap();
    println!(
        "recovery_observation={}",
        serde_json::json!({
            "installed_bytes": fs::read_to_string(&f.dest).unwrap(),
            "installed": f.installed(), "history": entry,
            "backups": hw.list_backups("tool").unwrap()
        })
    );
    assert!(!entry.ok);
    assert_eq!(entry.to, sha);
    assert!(
        fs::read_to_string(entry.log_path.as_ref().unwrap())
            .unwrap()
            .contains("verification")
    );
}

struct CancelOnOutput(CancellationToken);
impl EventSink for CancelOnOutput {
    fn emit(&self, event: UpdateEvent) {
        if matches!(
            event,
            UpdateEvent::Log {
                stream: LogStream::Stdout,
                ..
            }
        ) {
            self.0.cancel();
        }
    }
}

#[tokio::test]
async fn cancellation_during_verification_restores_previous_payload() {
    // Given an existing payload and a verifier that signals when it starts.
    let f = Fixture::new(None);
    f.place("old").await.unwrap();
    let old = f.installed();
    let app = with_github(f.app.clone(), |mut github| {
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.verify_command = Some("echo verifying; sleep 30".into());
        github
    });
    fs::write(&f.src, "new").unwrap();
    let cancel = CancellationToken::new();
    let sink = CancelOnOutput(cancel.clone());
    let log = RunLog::create(&f.hw.paths().app_log_dir("tool"), "tool", &sink).unwrap();
    let env = ShellEnv::current();
    // When cancellation arrives from the verifier's stdout event.
    let result = Replacement {
        app: &app,
        paths: f.hw.paths(),
        env: &env,
        log: &log,
        cancel: &cancel,
    }
    .place(&f.src, None, &f.dest)
    .await;
    // Then cancellation is reported only after old bytes have returned.
    assert!(matches!(result, Err(crate::Error::Cancelled)));
    assert_eq!(fs::read(&f.dest).unwrap(), b"old");
    assert_eq!(f.installed(), old);
}
