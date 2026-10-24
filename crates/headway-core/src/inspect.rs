//! Looking at a repository and proposing a config for it.
//!
//! `headway add owner/repo` and the app's "Inspect" button both start from this,
//! so a repo with a recognisable build or a macOS release asset needs no flags.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::asset::{self, Arch};
use crate::config::{AppKind, AssetInstall, Install, SourceInstall, Track, TrackedApp};
use crate::error::Result;
use crate::github::{GitHub, Release};
use crate::presets::{self, Preset};
use crate::repo::{RepoId, slugify};

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseSummary {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<DateTime<Utc>>,
    pub prerelease: bool,
    pub assets: Vec<String>,
    /// The asset Headway would download on this Mac.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_asset: Option<String>,
    /// Whether that asset holds an app bundle or a CLI binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_kind: Option<AppKind>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoInspection {
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub default_branch: String,
    pub private: bool,
    pub archived: bool,
    pub html_url: String,
    /// Preset id of the detected build system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_system: Option<String>,
    /// Binary names found in the build manifest.
    pub binaries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_release: Option<ReleaseSummary>,
    /// A config ready to save.
    pub suggestion: TrackedApp,
    /// Why the suggestion looks the way it does.
    pub notes: Vec<String>,
}

pub async fn inspect(gh: &GitHub, input: &str) -> Result<RepoInspection> {
    let repo = RepoId::parse(input)?;
    let info = gh.repo(&repo).await?;
    let releases = gh.releases(&repo).await?;
    // Empty repositories have no contents; that only means "nothing detected".
    let entries = gh.root_entries(&repo).await.unwrap_or_default();
    let preset = presets::detect(&entries);
    let binaries = match preset.map(|preset| preset.id) {
        Some("cargo" | "tauri") => gh
            .file_text(&repo, "Cargo.toml")
            .await?
            .map(|manifest| presets::cargo_binaries(&manifest))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let xcode_scheme = entries
        .iter()
        .find_map(|entry| entry.name.strip_suffix(".xcodeproj").map(str::to_owned));

    let latest = latest_stable(&releases);
    let (asset_kind, suggested_asset) = latest.map_or((AppKind::Cli, None), mac_asset);
    let latest_release = latest.map(|release| ReleaseSummary {
        tag: release.tag_name.clone(),
        name: release.name.clone().filter(|name| !name.trim().is_empty()),
        published_at: release.published_at,
        prerelease: release.prerelease,
        assets: release
            .assets
            .iter()
            .map(|asset| asset.name.clone())
            .collect(),
        suggested_kind: suggested_asset.as_ref().map(|_| asset_kind),
        suggested_asset: suggested_asset.clone(),
    });

    let facts = Facts {
        repo: &repo,
        default_branch: &info.default_branch,
        preset,
        binaries: &binaries,
        xcode_scheme: xcode_scheme.as_deref(),
        release: latest_release.as_ref(),
        asset_kind,
    };
    let (suggestion, notes) = suggest(&facts);
    Ok(RepoInspection {
        repo: repo.full_name(),
        description: info.description.filter(|text| !text.trim().is_empty()),
        default_branch: info.default_branch,
        private: info.private,
        archived: info.archived,
        html_url: info.html_url,
        build_system: preset.map(|preset| preset.id.to_owned()),
        binaries,
        latest_release,
        suggestion,
        notes,
    })
}

fn latest_stable(releases: &[Release]) -> Option<&Release> {
    releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .max_by_key(|release| release.published_at.or(release.created_at))
}

/// Which kind of payload the release ships for this Mac, and the asset name.
fn mac_asset(release: &Release) -> (AppKind, Option<String>) {
    let arch = Arch::host();
    [AppKind::App, AppKind::Cli]
        .into_iter()
        .find_map(|kind| {
            let chosen = asset::select(&release.assets, kind, None, arch).ok()?;
            let name = chosen.name.to_ascii_lowercase();
            let looks_like_app = std::path::Path::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dmg"))
                || name.contains(".app");
            (kind == AppKind::Cli || looks_like_app).then(|| (kind, Some(chosen.name.clone())))
        })
        .unwrap_or((AppKind::Cli, None))
}

struct Facts<'a> {
    repo: &'a RepoId,
    default_branch: &'a str,
    preset: Option<&'static Preset>,
    binaries: &'a [String],
    xcode_scheme: Option<&'a str>,
    release: Option<&'a ReleaseSummary>,
    asset_kind: AppKind,
}

fn suggest(facts: &Facts<'_>) -> (TrackedApp, Vec<String>) {
    let repo = facts.repo;
    let bin = facts
        .binaries
        .first()
        .cloned()
        .unwrap_or_else(|| repo.name.clone());
    let base = |kind, track, install| TrackedApp {
        id: slugify(&repo.name),
        name: repo.name.clone(),
        repo: repo.full_name(),
        kind,
        auto_update: false,
        git_url: None,
        track,
        install,
    };
    let mut notes = Vec::new();
    let mac_asset = facts.release.and_then(|release| {
        release
            .suggested_asset
            .as_ref()
            .map(|asset| (release.tag.as_str(), asset.as_str()))
    });

    let app = match (
        facts.preset.filter(|preset| preset.id != "custom"),
        mac_asset,
    ) {
        (Some(preset), _) => {
            let name = facts.xcode_scheme.unwrap_or(&repo.name);
            notes.push(format!(
                "Detected a {} build; following every commit on `{}`.",
                preset.label, facts.default_branch
            ));
            if facts.binaries.len() > 1 {
                notes.push(format!(
                    "The manifest declares several binaries ({}); installing `{bin}`.",
                    facts.binaries.join(", ")
                ));
            }
            if let Some((tag, asset)) = mac_asset {
                notes.push(format!(
                    "Release {tag} also ships `{asset}`: choose \"Releases\" to download it instead of building."
                ));
            }
            base(
                preset.kind,
                Track::Commit { branch: None },
                Install::Source(SourceInstall {
                    build: presets::render_command(preset.build, &bin, name),
                    workdir: None,
                    artifact: preset
                        .artifact
                        .map(|artifact| presets::render(artifact, &bin, name)),
                    dest: None,
                }),
            )
        }
        (None, Some((tag, asset))) => {
            notes.push(format!(
                "No known build system; release {tag} has a macOS asset (`{asset}`), so Headway follows releases."
            ));
            base(
                facts.asset_kind,
                Track::Release {
                    prerelease: false,
                    tag_filter: None,
                },
                Install::Asset(AssetInstall {
                    pattern: None,
                    binary: (facts.asset_kind == AppKind::Cli && bin != repo.name)
                        .then(|| bin.clone()),
                    dest: None,
                }),
            )
        }
        (None, None) => {
            notes.push(
                "Could not detect how to build this repository and it has no macOS release asset: set a build command."
                    .to_owned(),
            );
            base(
                AppKind::Cli,
                Track::Commit { branch: None },
                Install::Source(SourceInstall::default()),
            )
        }
    };
    (app, notes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn mock_repo(
        server: &MockServer,
        entries: serde_json::Value,
        releases: serde_json::Value,
    ) {
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
        assert_eq!(app.track, Track::Commit { branch: None });
        match &app.install {
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
            app.track,
            Track::Release {
                prerelease: false,
                ..
            }
        ));
        assert!(matches!(app.install, Install::Asset(_)));
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
}
