use super::*;
use crate::{
    Install, RepositoryOption, SourceInstall,
    testing::{cli_app, with_github},
};
use serde_json::json;
use std::os::unix::fs::PermissionsExt as _;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[test]
fn switching_back_restores_each_recipe_and_branch_without_relabelling_payloads() {
    // Given a known installed payload and two origins with different build recipes.
    let root = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .build()
        .unwrap();
    let payload = root.path().join("tool");
    std::fs::write(&payload, "#!/bin/sh\necho original\n").unwrap();
    std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o755)).unwrap();
    hw.add(with_github(cli_app("tool"), |mut github| {
        github.track = Track::Commit {
            branch: Some("main".into()),
        };
        github.install = Install::Source(SourceInstall {
            build: "make original".into(),
            artifact: Some("tool".into()),
            ..Default::default()
        });
        github.options.repositories.push(RepositoryOption {
            repo: "fork/tool".into(),
            branch: Some("feature".into()),
            ..Default::default()
        });
        github
    }))
    .unwrap();
    hw.store()
        .update_state(|state| {
            Ok((
                state.with_app("tool", |state| {
                    state.installed_now(InstalledRef {
                        id: "a".repeat(40),
                        sha: Some("a".repeat(40)),
                        version: None,
                        installed_at: Utc::now(),
                        path: Some(payload.display().to_string()),
                    })
                }),
                (),
            ))
        })
        .unwrap();
    let switched = hw.select_repository("tool", "fork/tool").unwrap();
    let edited = with_github(switched.app, |mut github| {
        github.track = Track::Commit {
            branch: Some("other".into()),
        };
        if let Install::Source(build) = &mut github.install {
            build.build = "make fork".into();
        }
        github
    });
    hw.replace(edited).unwrap();
    hw.select_repository("tool", "owner/tool").unwrap();
    // When returning to the fork after another round trip.
    let view = hw.select_repository("tool", "fork/tool").unwrap();
    // Then its recipe and branch survive, while original bytes stay separately attributed.
    let github = view.app.github().unwrap();
    assert_eq!(
        github.track,
        Track::Commit {
            branch: Some("other".into())
        }
    );
    let Install::Source(build) = &github.install else {
        panic!("source recipe")
    };
    assert_eq!(build.build, "make fork");
    assert_eq!(build.dest.as_deref(), Some(payload.to_str().unwrap()));
    assert_eq!(view.status, AppStatus::Unknown);
    assert!(view.installed.is_none());
    let detection = view.installation.unwrap();
    assert_eq!(detection.recorded.unwrap().repo, "owner/tool");
    assert!(!detection.baseline_matches);
    assert!(!view.app.auto_update);
    assert_eq!(
        std::fs::read_to_string(payload).unwrap(),
        "#!/bin/sh\necho original\n"
    );
}

#[tokio::test]
async fn fork_inspection_and_refresh_discover_parent_and_root_without_activation() {
    // Given a fork-of-fork and a pre-existing saved custom origin.
    let root = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let info = json!({"full_name":"owner/tool","default_branch":"feature","html_url":"https://github.com/owner/tool","fork":true,
        "parent":{"full_name":"parent/tool","default_branch":"develop"},"source":{"full_name":"root/tool","default_branch":"main"}});
    Mock::given(path("/repos/owner/tool"))
        .respond_with(ResponseTemplate::new(200).set_body_json(info))
        .mount(&server)
        .await;
    for endpoint in ["releases", "contents"] {
        Mock::given(path(format!("/repos/owner/tool/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
    }
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .api(server.uri())
        .secrets(Arc::new(crate::token::MemoryStore::default()))
        .env(ShellEnv::current())
        .build()
        .unwrap();
    hw.set_setting("gh_fallback", "false").unwrap();
    let inspected = hw.inspect("owner/tool").await.unwrap();
    assert!(inspected.fork);
    let choices = &inspected.suggestion.github().unwrap().options.repositories;
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.repo.as_str())
            .collect::<Vec<_>>(),
        ["owner/tool", "parent/tool", "root/tool"]
    );
    hw.add(with_github(cli_app("tool"), |mut github| {
        github.options.repositories.push(RepositoryOption {
            repo: "parent/tool".into(),
            label: "Custom parent".into(),
            branch: Some("custom".into()),
            ..Default::default()
        });
        github
    }))
    .unwrap();
    // When refreshing known fork ancestry.
    let view = hw.refresh_repositories("tool").await.unwrap();
    // Then the active origin and saved custom branch remain, with one added root choice.
    assert_eq!(view.app.repo_id().unwrap().full_name(), "owner/tool");
    let choices = hw.repositories("tool").unwrap();
    assert_eq!(choices.len(), 3);
    assert_eq!(
        choices
            .iter()
            .find(|choice| choice.repo == "parent/tool")
            .unwrap()
            .branch
            .as_deref(),
        Some("custom")
    );
}
