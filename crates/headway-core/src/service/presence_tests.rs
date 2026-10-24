use super::*;
use crate::config::{Install, SourceInstall};
use crate::events::NullSink;
use crate::testing::{GitFixture, cli_app, with_github};
use crate::token::MemoryStore;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

struct Fixture {
    _root: tempfile::TempDir,
    _repo: GitFixture,
    _server: MockServer,
    hw: Headway,
    dest: std::path::PathBuf,
    sha: String,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = GitFixture::new();
        let sha = repo.commit("payload", "#!/bin/sh\necho fixture\n", "payload");
        let server = MockServer::start().await;
        let commit = serde_json::json!({"sha": sha, "commit": {"message": "payload"}});
        for reference in ["main", sha.as_str()] {
            Mock::given(path(format!("/repos/owner/tool/commits/{reference}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(&commit))
                .mount(&server)
                .await;
        }
        Mock::given(path("/repos/owner/tool/commits"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([commit])))
            .mount(&server)
            .await;
        let hw = Headway::builder()
            .paths(Paths::under(root.path().join("store")))
            .secrets(Arc::new(MemoryStore::default()))
            .env(ShellEnv::current())
            .api(server.uri())
            .build()
            .unwrap();
        hw.set_setting("gh_fallback", "false").unwrap();
        let dest = root.path().join("installed/tool");
        hw.add(with_github(cli_app("tool"), |mut github| {
            github.git_url = Some(repo.url());
            github.track = Track::Commit {
                branch: Some("main".into()),
            };
            github.install = Install::Source(SourceInstall {
                build: "test -f payload".into(),
                artifact: Some("payload".into()),
                dest: Some(dest.display().to_string()),
                ..SourceInstall::default()
            });
            github
        }))
        .unwrap();
        Self {
            _root: root,
            _repo: repo,
            _server: server,
            hw,
            dest,
            sha,
        }
    }

    async fn install_then_delete(&self) {
        self.hw
            .update(
                "tool",
                &Target::Ref(self.sha.clone()),
                &NullSink,
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(self.dest.is_file());
        std::fs::remove_file(&self.dest).unwrap();
    }
}

#[tokio::test]
async fn missing_managed_payload_is_not_reported_installed_by_views_checks_or_plans() {
    // Given a managed payload removed outside Headway, with its SHA still recorded.
    let fixture = Fixture::new().await;
    fixture.install_then_delete().await;
    // When reading, checking and planning that app.
    let view = fixture.hw.get("tool").unwrap();
    let checked = fixture.hw.check("tool").await.unwrap();
    let plan = fixture.hw.plan("tool", &Target::Latest).await.unwrap();
    println!(
        "missing_presence={}",
        serde_json::json!({"view":view.status,"check":checked.status,"plan_up_to_date":plan.up_to_date})
    );
    // Then absence wins over the cached SHA and the latest plan cannot skip installation.
    assert_eq!(view.status, AppStatus::NotInstalled);
    assert!(view.installed.is_none());
    assert_eq!(checked.status, AppStatus::NotInstalled);
    assert!(checked.installed.is_none());
    assert!(!plan.up_to_date);
    assert!(plan.from.is_none());
}

#[tokio::test]
async fn latest_reinstalls_missing_payload_even_when_recorded_sha_matches() {
    // Given the recorded latest SHA but a physically deleted managed payload.
    let fixture = Fixture::new().await;
    fixture.install_then_delete().await;
    // When requesting latest, without an explicit SHA override.
    let outcome = fixture
        .hw
        .update(
            "tool",
            &Target::Latest,
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    println!(
        "reinstall_presence={}",
        serde_json::json!({"changed":outcome.changed,"payload_exists":fixture.dest.is_file(),"sha_matches":outcome.to.sha.as_deref()==Some(fixture.sha.as_str())})
    );
    // Then a real install restores the payload despite the matching recorded revision.
    assert!(outcome.changed);
    assert_eq!(
        std::fs::read_to_string(&fixture.dest).unwrap(),
        "#!/bin/sh\necho fixture\n"
    );
    assert_eq!(outcome.to.sha.as_deref(), Some(fixture.sha.as_str()));
    assert_eq!(fixture.hw.get("tool").unwrap().status, AppStatus::UpToDate);
}

#[tokio::test]
async fn external_literal_destination_is_unknown_without_inventing_a_baseline() {
    use std::os::unix::fs::PermissionsExt as _;
    // Given an executable at the explicit destination, without a Headway baseline.
    let fixture = Fixture::new().await;
    std::fs::create_dir_all(fixture.dest.parent().unwrap()).unwrap();
    std::fs::write(&fixture.dest, "#!/bin/sh\nexit 99\n").unwrap();
    std::fs::set_permissions(&fixture.dest, std::fs::Permissions::from_mode(0o755)).unwrap();
    // When listing and checking, no command from that installed file should run.
    let view = fixture.hw.get("tool").unwrap();
    let checked = fixture.hw.check("tool").await.unwrap();
    // Then presence is shown as unknown revision, never assigned the upstream head SHA.
    assert_eq!(view.status, AppStatus::Unknown);
    assert_eq!(checked.status, AppStatus::Unknown);
    assert!(view.installed.is_none());
    assert!(checked.installed.is_none());
    assert!(
        fixture
            .hw
            .store
            .state()
            .unwrap()
            .app("tool")
            .installed
            .is_none()
    );
}
