use super::*;
use crate::Target;
use crate::testing::GitFixture;
use crate::token::MemoryStore;
use std::sync::Arc;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[tokio::test]
async fn public_update_after_repointing_ignores_foreign_backups_and_records_only_new_repo_sha() {
    // Given source A's real retained payload and a source B repository/API fixture.
    let f = Fixture::new(None);
    f.place("a-old").await.unwrap();
    f.place("a-current").await.unwrap();
    let foreign = f.hw.list_backups("tool").unwrap().remove(0);
    let repo = GitFixture::new();
    let sha = repo.commit("payload", "source-b", "source B");
    let server = MockServer::start().await;
    Mock::given(path(format!("/repos/other/tool/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": sha, "commit": {"message": "source B"}
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
    let app = with_github(f.app.clone(), |mut github| {
        github.repo = "other/tool".into();
        github.git_url = Some(repo.url());
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.build = "test -f payload".into();
        github
    });
    hw.replace(app).unwrap();
    // When the public update API installs source B at the shared destination.
    let outcome = hw
        .update(
            "tool",
            &Target::Ref(sha.clone()),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    // Then foreign backups do not block the build or assign source A's SHA to B.
    assert_eq!(outcome.to.id, sha);
    assert!(outcome.from.is_none());
    assert_eq!(f.installed().unwrap().id, sha);
    assert_eq!(fs::read(&f.dest).unwrap(), b"source-b");
    assert!(
        hw.restore_backup("tool", &foreign.id, &NullSink, &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(f.installed().unwrap().id, sha);
    let backups = hw.list_backups("tool").unwrap();
    assert_eq!(backups.len(), 1);
    assert!(
        backups[0].installed.is_none(),
        "unattributed prior bytes must never acquire A's SHA"
    );
    println!(
        "public_repoint_update={}",
        serde_json::json!({"outcome":outcome,"backups":backups,"bytes":fs::read_to_string(&f.dest).unwrap()})
    );
}
