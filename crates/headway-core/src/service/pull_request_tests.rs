use super::*;
use crate::testing::{GitFixture, cli_app, with_github};
use crate::{Install, NullSink, RevisionMethod, RevisionRequest, SourceInstall, VariantRequest};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{path, query_param},
};

struct Fixture {
    root: tempfile::TempDir,
    _repo: GitFixture,
    server: MockServer,
    hw: Headway,
    sha: String,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = GitFixture::new();
        let sha = repo.commit("payload", "#!/bin/sh\necho pr-fixture\n", "PR payload");
        // This commit is reachable only through GitHub's PR namespace.
        repo.git(&["update-ref", "refs/pull/7/head", &sha]);
        repo.git(&["reset", "--hard", "HEAD~1"]);
        let server = MockServer::start().await;
        let hw = Headway::builder()
            .paths(Paths::under(root.path()))
            .secrets(Arc::new(crate::token::MemoryStore::default()))
            .env(ShellEnv::current())
            .api(server.uri())
            .build()
            .unwrap();
        hw.set_setting("gh_fallback", "false").unwrap();
        hw.set_setting("bin_dir", root.path().join("bin").to_str().unwrap())
            .unwrap();
        let mut app = with_github(cli_app("tool"), |mut github| {
            github.git_url = Some(repo.url());
            github.install = Install::Source(SourceInstall {
                build: "test -z \"$HEADWAY_GITHUB_TOKEN\" && cp payload tool && chmod +x tool"
                    .into(),
                artifact: Some("tool".into()),
                dest: Some(root.path().join("bin/tool").display().to_string()),
                ..SourceInstall::default()
            });
            github
        });
        app.auto_update = true;
        hw.add(app).unwrap();
        Mock::given(path(format!("/repos/owner/tool/commits/{sha}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"sha":sha,"commit":{"message":"PR payload"}})),
            )
            .mount(&server)
            .await;
        let fixture = Self {
            root,
            _repo: repo,
            server,
            hw,
            sha,
        };
        fixture.head(&fixture.sha).await;
        fixture
    }

    fn body(sha: &str) -> serde_json::Value {
        json!({"number":7,"title":"Fork contribution","html_url":"https://github.com/owner/tool/pull/7","state":"closed","head":{"ref":"feature","sha":sha,"repo":null},"base":{"ref":"main","sha":"b".repeat(40),"repo":{"full_name":"owner/tool"}}})
    }

    async fn head(&self, sha: &str) {
        Mock::given(path("/repos/owner/tool/pulls/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Self::body(sha)))
            .with_priority(if sha == self.sha { 10 } else { 1 })
            .mount(&self.server)
            .await;
    }

    fn request(&self) -> RevisionRequest {
        RevisionRequest {
            reference: format!("pr:7@{}", self.sha),
            method: RevisionMethod::Configured,
            release_tag: None,
            source_build: None,
            release_asset: None,
        }
    }
}

#[tokio::test]
async fn pr_only_commit_installs_even_when_fork_is_deleted_and_disables_auto_update() {
    // Given a closed PR whose fork no longer exists and head is outside all branches.
    let fixture = Fixture::new().await;
    // When installing the reviewed head through the public revision flow.
    let outcome = fixture
        .hw
        .install_revision(
            "tool",
            &fixture.request(),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    // Then exactly that payload and PR provenance are installed, without changing origins.
    assert_eq!(outcome.to.sha.as_deref(), Some(fixture.sha.as_str()));
    assert_eq!(
        std::fs::read_to_string(fixture.root.path().join("bin/tool")).unwrap(),
        "#!/bin/sh\necho pr-fixture\n"
    );
    let view = fixture.hw.get("tool").unwrap();
    assert!(!view.app.auto_update);
    assert_eq!(view.app.repo_id().unwrap().full_name(), "owner/tool");
    assert_eq!(view.pull_request.unwrap().number, 7);
    assert!(view.installation.unwrap().baseline_matches);
}

#[tokio::test]
async fn changed_pr_head_is_rejected_before_configuration_or_payload_changes() {
    // Given a pinned review followed by an upstream head change.
    let fixture = Fixture::new().await;
    fixture.head(&"c".repeat(40)).await;
    let config = std::fs::read(fixture.hw.paths().config_file()).unwrap();
    // When installing the old review.
    let error = fixture
        .hw
        .install_revision(
            "tool",
            &fixture.request(),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    // Then it cannot silently follow the moved PR or disable auto-update on a rejected request.
    assert_eq!(error.code(), "invalid_input");
    assert_eq!(
        std::fs::read(fixture.hw.paths().config_file()).unwrap(),
        config
    );
    assert!(!fixture.root.path().join("bin/tool").exists());
}

#[tokio::test]
async fn independent_pr_install_preserves_parent_and_records_resolved_pin() {
    // Given an existing parent installation.
    let fixture = Fixture::new().await;
    let path = fixture.root.path().join("bin/tool");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "parent").unwrap();
    let request = VariantRequest {
        id: "tool-pr7".into(),
        name: None,
        revision: fixture.request(),
    };
    // When installing a PR alongside it.
    let outcome = fixture
        .hw
        .install_variant("tool", &request, &NullSink, &CancellationToken::new())
        .await
        .unwrap();
    // Then the parent bytes remain and the child records its exact PR and SHA.
    assert_eq!(std::fs::read_to_string(path).unwrap(), "parent");
    assert_eq!(outcome.to.sha.as_deref(), Some(fixture.sha.as_str()));
    let child = fixture.hw.get("tool-pr7").unwrap();
    assert_eq!(
        child
            .app
            .github()
            .unwrap()
            .options
            .variant
            .as_ref()
            .unwrap()
            .reference,
        fixture.sha
    );
    assert_eq!(child.pull_request.unwrap().number, 7);
}

#[tokio::test]
async fn pr_pagination_and_source_only_contract_are_explicit() {
    // Given an API page and a PR with no release artifact contract.
    let fixture = Fixture::new().await;
    Mock::given(path("/repos/owner/tool/pulls"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([Fixture::body(&fixture.sha)])),
        )
        .mount(&fixture.server)
        .await;
    // When reading page two through the public API.
    let page = fixture.hw.list_pull_requests("tool", 2, 1).await.unwrap();
    // Then page boundaries are preserved and a next page is available.
    assert_eq!(page.pull_requests[0].head.sha, fixture.sha);
    assert!(page.has_more);
}

#[tokio::test]
async fn release_asset_cannot_be_installed_as_a_pr_head() {
    // Given a reviewed PR and an asset request.
    let fixture = Fixture::new().await;
    let request = RevisionRequest {
        method: RevisionMethod::Asset,
        ..fixture.request()
    };
    // When planning that incompatible request.
    let error = fixture
        .hw
        .plan_revision("tool", &request)
        .await
        .unwrap_err();
    // Then rejection happens before any install.
    assert_eq!(error.code(), "invalid_input");
    assert!(!fixture.root.path().join("bin/tool").exists());
}

#[tokio::test]
async fn invalid_pr_pin_is_rejected_without_remote_access() {
    // Given syntactically invalid PR references.
    let fixture = Fixture::new().await;
    for reference in ["pr:0", "pr:-1", "pr:7@short", "pr:nope"] {
        // When resolving one reference.
        let error = fixture
            .hw
            .revision_options("tool", reference)
            .await
            .unwrap_err();
        // Then the machine contract reports invalid input.
        assert_eq!(error.code(), "invalid_input");
    }
    assert!(fixture.server.received_requests().await.unwrap().is_empty());
}
