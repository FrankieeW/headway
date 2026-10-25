//! Editable app configurations, separate from build-system presets.
use serde::{Deserialize, Serialize};

use crate::{
    AppKind, BrewSource, GithubSource, Install, NpmSource, Source, SourceInstall, Track, TrackedApp,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRecipe {
    pub id: String,
    pub name: String,
    pub description: String,
    pub app: TrackedApp,
}

/// Common source configurations. No commands execute while listing recipes.
#[must_use]
pub fn recipes() -> Vec<AppRecipe> {
    let entries = [
        (
            "ripgrep-brew",
            "ripgrep",
            "Track the Homebrew ripgrep formula.",
            AppKind::Cli,
            Source::Brew(BrewSource {
                install_path: None,
                package: "ripgrep".into(),
                cask: false,
            }),
        ),
        (
            "jq-brew",
            "jq",
            "Track the Homebrew jq formula.",
            AppKind::Cli,
            Source::Brew(BrewSource {
                install_path: None,
                package: "jq".into(),
                cask: false,
            }),
        ),
        (
            "pnpm-npm",
            "pnpm",
            "Track the latest stable global npm package.",
            AppKind::Cli,
            Source::Npm(NpmSource {
                package: "pnpm".into(),
                dist_tag: "latest".into(),
            }),
        ),
        (
            "headway-app",
            "Headway",
            "Build the menu-bar app and its bundled CLI from main using Headway's build helper.",
            AppKind::App,
            Source::Github(GithubSource {
                options: Box::default(),
                repo: "FrankieeW/headway".into(),
                git_url: None,
                track: Track::Commit {
                    branch: Some("main".into()),
                },
                install: Install::Source(SourceInstall {
                    build: "./scripts/build-headway.sh app".into(),
                    artifact: Some(".headway-out/Headway.app".into()),
                    ..SourceInstall::default()
                }),
            }),
        ),
    ];
    entries
        .into_iter()
        .map(|(id, name, description, kind, source)| AppRecipe {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            app: TrackedApp {
                id: id.into(),
                name: name.into(),
                kind,
                auto_update: false,
                update_policy: crate::config::UpdatePolicy::default(),
                source,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn recipes_are_valid_editable_configs_with_safe_defaults() {
        // Given the built-in recipe catalog.
        let recipes = super::recipes();
        // When serializing and validating each config.
        for recipe in recipes {
            let json = serde_json::to_string(&recipe).unwrap();
            let decoded: super::AppRecipe = serde_json::from_str(&json).unwrap();
            decoded.app.validate().unwrap();
            // Then no recipe enables unattended updates or a personal destination.
            assert!(!decoded.app.auto_update);
            if let Some(github) = decoded.app.github() {
                match &github.install {
                    crate::Install::Source(source) => assert!(source.dest.is_none()),
                    crate::Install::Asset(asset) => assert!(asset.dest.is_none()),
                }
            }
        }
    }
}
