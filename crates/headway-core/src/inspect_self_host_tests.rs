use super::*;

async fn tauri_repo(server: &MockServer, repo: &str, manifest: Option<&str>) {
    Mock::given(path(format!("/repos/{repo}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name": repo, "default_branch": "main",
            "html_url": format!("https://github.com/{repo}")
        })))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(path(format!("/repos/{repo}/releases")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1, "tag_name": "v1", "assets": [
                {"id": 1, "name": "Headway.dmg", "size": 1}
            ]
        }])))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(path(format!("/repos/{repo}/contents")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "Cargo.toml", "type": "file"},
            {"name": "src-tauri", "type": "dir"},
            {"name": "scripts", "type": "dir"}
        ])))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(path(format!("/repos/{repo}/contents/Cargo.toml")))
        .respond_with(manifest.map_or_else(
            || ResponseTemplate::new(404),
            |text| ResponseTemplate::new(200).set_body_string(text),
        ))
        .expect(1)
        .mount(server)
        .await;
}

fn source_install(inspection: &RepoInspection) -> &SourceInstall {
    match &inspection.suggestion.github().unwrap().install {
        Install::Source(source) => source,
        other @ Install::Asset(_) => panic!("unexpected {other:?}"),
    }
}

const WORKSPACE: &str = "[workspace]\nmembers = ['crates/core', 'src-tauri']";

#[tokio::test]
async fn official_headway_with_script_prefers_app_and_default_branch_commits() {
    for repo in ["FrankieeW/headway", "frankieew/HEADWAY"] {
        let server = MockServer::start().await;
        tauri_repo(&server, repo, Some(WORKSPACE)).await;
        Mock::given(path(format!(
            "/repos/{repo}/contents/scripts/build-headway.sh"
        )))
        .and(wiremock::matchers::header(
            "accept",
            "application/vnd.github.raw+json",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("#!/bin/sh\ncase $1 in app) build_app;; cli) build_cli;; esac\n"),
        )
        .expect(1)
        .mount(&server)
        .await;

        let gh = GitHub::new(&server.uri(), None).unwrap();
        let found = inspect(&gh, repo).await.unwrap();

        assert_eq!(found.build_system.as_deref(), Some("headway-app"));
        assert_eq!(found.suggestion.id, "headway");
        assert_eq!(found.suggestion.name, "Headway");
        assert_eq!(found.suggestion.kind, AppKind::App);
        assert_eq!(found.default_branch, "main");
        assert_eq!(
            found.suggestion.github().unwrap().track,
            Track::Commit { branch: None }
        );
        assert_eq!(
            source_install(&found).build,
            "./scripts/build-headway.sh app"
        );
        assert_eq!(
            source_install(&found).artifact.as_deref(),
            Some(".headway-out/Headway.app")
        );
        assert!(found.binaries.is_empty());
        assert_eq!(
            found.latest_release.unwrap().suggested_asset.as_deref(),
            Some("Headway.dmg")
        );
        found.suggestion.validate().unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 5);
    }
}

#[tokio::test]
async fn official_headway_before_script_exists_keeps_generic_workspace_tauri_build() {
    for response in [
        ResponseTemplate::new(404),
        ResponseTemplate::new(200).set_body_string(" \n"),
    ] {
        let server = MockServer::start().await;
        let repo = "FrankieeW/headway";
        tauri_repo(&server, repo, Some(WORKSPACE)).await;
        Mock::given(path(format!(
            "/repos/{repo}/contents/scripts/build-headway.sh"
        )))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;

        let gh = GitHub::new(&server.uri(), None).unwrap();
        let found = inspect(&gh, repo).await.unwrap();

        assert_eq!(found.build_system.as_deref(), Some("tauri"));
        assert_eq!(
            source_install(&found).build,
            presets::find("tauri").unwrap().build
        );
        assert_eq!(
            source_install(&found).artifact.as_deref(),
            Some("target/release/bundle/macos/*.app")
        );
        found.suggestion.validate().unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 5);
    }
}

#[tokio::test]
async fn unrelated_tauri_repos_keep_artifact_layout_without_probing_headway_script() {
    for (repo, manifest, artifact) in [
        (
            "someone/headway",
            Some(WORKSPACE),
            "target/release/bundle/macos/*.app",
        ),
        (
            "FrankieeW/other",
            Some(WORKSPACE),
            "target/release/bundle/macos/*.app",
        ),
        (
            "someone/standalone",
            None,
            "src-tauri/target/release/bundle/macos/*.app",
        ),
        (
            "someone/package",
            Some("[package]\nname = 'root'"),
            "src-tauri/target/release/bundle/macos/*.app",
        ),
        (
            "someone/other-workspace",
            Some("[workspace]\nmembers = ['crates/core']"),
            "src-tauri/target/release/bundle/macos/*.app",
        ),
    ] {
        let server = MockServer::start().await;
        tauri_repo(&server, repo, manifest).await;

        let gh = GitHub::new(&server.uri(), None).unwrap();
        let found = inspect(&gh, repo).await.unwrap();

        assert_eq!(found.build_system.as_deref(), Some("tauri"));
        assert_eq!(source_install(&found).artifact.as_deref(), Some(artifact));
        found.suggestion.validate().unwrap();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 4);
        assert!(
            requests
                .iter()
                .all(|request| !request.url.path().contains("build-headway"))
        );
    }
}
