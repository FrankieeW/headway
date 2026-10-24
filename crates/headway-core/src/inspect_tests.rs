use serde_json::json;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

async fn mock_repo(server: &MockServer, entries: serde_json::Value, releases: serde_json::Value) {
    Mock::given(path("/repos/BurntSushi/ripgrep"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "full_name": "BurntSushi/ripgrep", "description": "fast grep", "default_branch": "master",
                "private": false, "archived": false, "html_url": "https://github.com/BurntSushi/ripgrep"
            })))
            .mount(server)
            .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(releases))
        .mount(server)
        .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(entries))
        .mount(server)
        .await;
    Mock::given(path("/repos/BurntSushi/ripgrep/contents/Cargo.toml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "[package]\nname = \"ripgrep\"\n[[bin]]\nname = \"rg\"\npath = \"main.rs\"\n",
        ))
        .mount(server)
        .await;
}

fn releases() -> serde_json::Value {
    json!([{
        "id": 1, "tag_name": "14.1.1", "name": "", "draft": false, "prerelease": false,
        "published_at": "2026-01-01T00:00:00Z",
        "assets": [
            {"id": 10, "name": "ripgrep-14.1.1-aarch64-apple-darwin.tar.gz", "size": 1},
            {"id": 11, "name": "ripgrep-14.1.1-x86_64-apple-darwin.tar.gz", "size": 1},
            {"id": 12, "name": "ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz", "size": 1}
        ]
    }])
}

#[tokio::test]
async fn cargo_repo_suggests_commit_tracking_with_the_real_binary_name() {
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name": "Cargo.toml", "type": "file"}, {"name": "crates", "type": "dir"}]),
        releases(),
    )
    .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let found = inspect(&gh, "https://github.com/BurntSushi/ripgrep")
        .await
        .unwrap();

    assert_eq!(found.build_system.as_deref(), Some("cargo"));
    assert_eq!(found.binaries, ["rg"]);
    assert_eq!(found.description.as_deref(), Some("fast grep"));
    let app = &found.suggestion;
    assert_eq!(app.id, "ripgrep");
    assert_eq!(app.github().unwrap().track, Track::Commit { branch: None });
    match &app.github().unwrap().install {
        Install::Source(source) => {
            assert_eq!(source.build, "cargo build --release --locked");
            assert_eq!(source.artifact.as_deref(), Some("target/release/rg"));
        }
        other @ Install::Asset(_) => panic!("unexpected {other:?}"),
    }
    app.validate().unwrap();
    let release = found.latest_release.unwrap();
    assert!(release.suggested_asset.unwrap().contains("apple-darwin"));
    assert!(found.notes.iter().any(|note| note.contains("Releases")));
}

#[tokio::test]
async fn unknown_build_with_mac_assets_follows_releases() {
    let server = MockServer::start().await;
    mock_repo(
        &server,
        json!([{"name": "README.md", "type": "file"}]),
        releases(),
    )
    .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    let app = &found.suggestion;
    assert!(matches!(
        app.github().unwrap().track,
        Track::Release {
            prerelease: false,
            ..
        }
    ));
    assert!(matches!(app.github().unwrap().install, Install::Asset(_)));
    assert_eq!(app.kind, AppKind::Cli);
    app.validate().unwrap();
}

#[tokio::test]
async fn nothing_detected_asks_for_a_build_command() {
    let server = MockServer::start().await;
    mock_repo(&server, json!([]), json!([])).await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let found = inspect(&gh, "BurntSushi/ripgrep").await.unwrap();
    assert!(found.latest_release.is_none());
    assert!(
        found.suggestion.validate().is_err(),
        "build command must be filled in"
    );
    assert!(found.notes[0].contains("build command"));
}

#[test]
fn dmg_releases_are_apps() {
    let release: Release = serde_json::from_value(json!({
        "id": 1, "tag_name": "v1", "assets": [
            {"id": 1, "name": "Ice.dmg", "size": 1},
            {"id": 2, "name": "Ice.zip", "size": 1}
        ]
    }))
    .unwrap();
    assert_eq!(
        mac_asset(&release),
        (AppKind::App, Some("Ice.dmg".to_owned()))
    );
}

#[path = "inspect_self_host_tests.rs"]
mod self_host;
