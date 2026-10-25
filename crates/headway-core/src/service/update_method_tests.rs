//! A commit-tracked app can be updated from the release that published the
//! commit: the method is forced for that run only and never rewrites the
//! app's own recipe.
use serde_json::{Value, json};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::Headway;
use crate::config::{AssetInstall, Track};
use crate::paths::Paths;
use crate::revision_install::RevisionMethod;
use crate::shell::ShellEnv;
use crate::testing::{cli_app, with_github};
use crate::token::MemoryStore;
use crate::update::{Target, UpdateRequest};

const RELEASED: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TAG: &str = "v1.2.3";
const ASSET: &str = "tool-v1.2.3.tar.gz";

/// A commit-tracked app that builds from source but knows a release recipe.
fn headway(server: &MockServer) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .secrets(std::sync::Arc::new(MemoryStore::with_token(
            "fixture-token",
        )))
        .env(ShellEnv::default())
        .api(server.uri())
        .build()
        .unwrap();
    hw.add(with_github(cli_app("selected-app"), |source| {
        let mut options = *source.options.clone();
        options.release_asset = Some(AssetInstall {
            pattern: Some("*.tar.gz".into()),
            ..AssetInstall::default()
        });
        crate::GithubSource {
            repo: "acme/project".into(),
            track: Track::Commit {
                branch: Some("main".into()),
            },
            options: Box::new(options),
            ..source
        }
    }))
    .unwrap();
    (dir, hw)
}

fn commit(sha: &str) -> Value {
    json!({"sha":sha,"html_url":format!("https://github.com/acme/project/commit/{sha}"),
        "commit":{"message":"Published","author":{"name":"Builder","date":"2026-10-01T00:00:00Z"},
            "committer":{"date":"2026-10-02T00:00:00Z"}}})
}

/// The commit, its tag and the release that published it.
async fn repository(server: &MockServer, with_release: bool) {
    Mock::given(path(format!("/repos/acme/project/commits/{RELEASED}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit(RELEASED)))
        .mount(server)
        .await;
    Mock::given(path("/repos/acme/project/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":TAG,"commit":{"sha":RELEASED}}
        ])))
        .mount(server)
        .await;
    Mock::given(path(format!("/repos/acme/project/commits/refs/tags/{TAG}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit(RELEASED)))
        .mount(server)
        .await;
    // The commit is not a tag: the lookup by tag name has to miss first.
    Mock::given(path(format!(
        "/repos/acme/project/releases/tags/{RELEASED}"
    )))
    .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
    .mount(server)
    .await;
    if with_release {
        Mock::given(path(format!("/repos/acme/project/releases/tags/{TAG}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id":7,"tag_name":TAG,"draft":false,"prerelease":false,
                "html_url":"https://github.com/acme/project/releases/tag/v1.2.3",
                "assets":[{"id":9,"name":ASSET,"size":10}]
            })))
            .mount(server)
            .await;
    } else {
        Mock::given(path(format!("/repos/acme/project/releases/tags/{TAG}")))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn a_commit_update_can_be_installed_from_its_release() {
    // Given a commit-tracked app whose commit published a release with an asset.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the update is planned with the release asset as the install method.
    let plan = hw
        .plan_with(
            "selected-app",
            &UpdateRequest {
                target: Target::Ref(RELEASED.to_owned()),
                method: RevisionMethod::Asset,
            },
        )
        .await
        .unwrap();
    // Then the plan downloads that release instead of building the commit.
    assert_eq!(plan.method, "asset");
    assert_eq!(
        plan.asset.as_ref().map(|asset| asset.name.as_str()),
        Some(ASSET)
    );
    assert!(
        plan.steps.iter().any(|step| step.contains(ASSET)),
        "{plan:?}"
    );
    assert!(
        !plan.steps.iter().any(|step| step.contains("cargo build")),
        "{plan:?}"
    );
}

#[tokio::test]
async fn the_saved_recipe_still_decides_the_install_method() {
    // Given the same app and release.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the update is planned without forcing a method.
    let plan = hw
        .plan_with(
            "selected-app",
            &UpdateRequest {
                target: Target::Ref(RELEASED.to_owned()),
                method: RevisionMethod::Configured,
            },
        )
        .await
        .unwrap();
    // Then the app's own source recipe is used.
    assert_eq!(plan.method, "source");
    assert!(plan.asset.is_none());
    assert!(
        plan.steps.iter().any(|step| step.contains("cargo build")),
        "{plan:?}"
    );
}

#[tokio::test]
async fn a_source_build_can_be_forced_for_a_release_commit() {
    // Given the same app and release.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, true).await;
    // When the update is planned with a source build.
    let plan = hw
        .plan_with(
            "selected-app",
            &UpdateRequest {
                target: Target::Ref(RELEASED.to_owned()),
                method: RevisionMethod::Source,
            },
        )
        .await
        .unwrap();
    // Then the commit is built from source even though a release exists.
    assert_eq!(plan.method, "source");
    assert!(plan.asset.is_none());
    assert!(
        plan.steps.iter().any(|step| step.contains("cargo build")),
        "{plan:?}"
    );
}

#[tokio::test]
async fn a_commit_without_a_release_cannot_be_installed_from_one() {
    // Given a commit that never published a release.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    repository(&server, false).await;
    // When the update is planned with the release asset as the install method.
    let plan = hw
        .plan_with(
            "selected-app",
            &UpdateRequest {
                target: Target::Ref(RELEASED.to_owned()),
                method: RevisionMethod::Asset,
            },
        )
        .await;
    // Then the plan refuses instead of silently building the commit.
    assert!(
        matches!(plan, Err(crate::Error::RemoteNotFound(_))),
        "{plan:?}"
    );
}

#[tokio::test]
async fn unsupported_methods_fail_before_probes_or_update_history() {
    // Given a non-GitHub app and no network or executable fixtures.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    hw.add(crate::TrackedApp {
        source: crate::Source::Npm(crate::NpmSource {
            package: "fixture".into(),
            dist_tag: "latest".into(),
        }),
        ..cli_app("npm-app")
    })
    .unwrap();
    let request = UpdateRequest {
        target: Target::Latest,
        method: RevisionMethod::Asset,
    };

    // When planning or starting an unsupported forced method.
    let plan = hw.plan_with("npm-app", &request).await;
    let update = hw
        .update_with(
            "npm-app",
            &request,
            &crate::NullSink,
            &crate::CancellationToken::new(),
        )
        .await;

    // Then both reject locally and the rejected admission leaves no failed run.
    assert!(
        matches!(plan, Err(crate::Error::InvalidInput(_))),
        "{plan:?}"
    );
    assert!(
        matches!(update, Err(crate::Error::InvalidInput(_))),
        "{update:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(hw.history("npm-app").unwrap().is_empty());
}

#[tokio::test]
async fn source_override_without_a_build_recipe_fails_before_network() {
    // Given an asset-only app.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(&server);
    let app = with_github(hw.get("selected-app").unwrap().app, |mut source| {
        source.install = crate::Install::Asset(AssetInstall::default());
        source
    });
    hw.replace(app).unwrap();
    let request = UpdateRequest {
        target: Target::Latest,
        method: RevisionMethod::Source,
    };

    // When a source build is requested without a saved build command.
    let plan = hw.plan_with("selected-app", &request).await;
    let update = hw
        .update_with(
            "selected-app",
            &request,
            &crate::NullSink,
            &crate::CancellationToken::new(),
        )
        .await;

    // Then planning and execution reject the same invalid request before fetching.
    assert!(matches!(plan, Err(crate::Error::InvalidInput(_))));
    assert!(matches!(update, Err(crate::Error::InvalidInput(_))));
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(hw.history("selected-app").unwrap().is_empty());
}
