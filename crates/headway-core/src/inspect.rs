//! Looking at a repository and proposing a config for it.
//!
//! `headway add owner/repo` and the app's "Inspect" button both start from this,
//! so a repo with a recognisable build or a macOS release asset needs no flags.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::asset::{self, Arch};
use crate::build_detection::fallback::{Facts, suggest};
use crate::build_detection::{self, Discovery};
pub use crate::build_detection::{BuildCandidate, BuildCandidateSource};
use crate::config::{AppKind, Install, SourceInstall, Track, TrackedApp};
use crate::error::Result;
use crate::github::{GitHub, Release};
use crate::presets;
use crate::repo::RepoId;
use crate::source::Source;

#[derive(Debug, Clone, Serialize)]
pub struct PackageLookup {
    pub source: &'static str,
    pub package: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub latest: Option<String>,
    pub installed: Option<String>,
    pub suggestion: TrackedApp,
    pub notes: Vec<String>,
}

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
    pub fork: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<crate::github::RelatedRepo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::github::RelatedRepo>,
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
    /// Editable source build suggestions; inspection never executes them.
    pub build_candidates: Vec<BuildCandidate>,
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
    let mut discovery = Discovery::new(gh, &repo);
    let entries = match gh.root_entries(&repo).await {
        Ok(entries) => entries,
        Err(err @ (crate::Error::Auth(_) | crate::Error::RateLimited { .. })) => return Err(err),
        Err(err) => {
            discovery
                .notes
                .push(format!("Could not inspect repository contents: {err}"));
            Vec::new()
        }
    };
    let headway_family = repo.full_name().eq_ignore_ascii_case("FrankieeW/headway")
        || info
            .parent
            .iter()
            .chain(&info.source)
            .any(|related| related.full_name.eq_ignore_ascii_case("FrankieeW/headway"));
    let known_self_layout = headway_family
        && ["scripts", "src-tauri"].iter().all(|name| {
            entries
                .iter()
                .any(|entry| entry.name == *name && entry.kind == "dir")
        });
    if known_self_layout {
        discovery.read("scripts/build-headway.sh").await?;
    }
    discovery.collect(&entries).await?;
    let mut preset = presets::detect(&entries);
    let manifest = discovery.files.get("Cargo.toml").cloned();
    let mut build_candidates = discovery.candidates(&entries);
    let binaries: Vec<String> = build_candidates
        .iter()
        .filter(|candidate| candidate.file.ends_with("Cargo.toml"))
        .filter_map(|candidate| {
            candidate
                .artifact
                .as_deref()?
                .strip_prefix("target/release/")
                .map(str::to_owned)
        })
        .collect();
    // A manifest filename alone does not establish a CLI or its output path.
    if preset.is_some_and(|preset| matches!(preset.id, "npm" | "cargo" | "go" | "swiftpm" | "make"))
    {
        preset = None;
    }
    if known_self_layout
        && discovery
            .files
            .get("scripts/build-headway.sh")
            .is_some_and(|script| !script.trim().is_empty())
    {
        preset = presets::find("headway-app");
    }
    let artifact = if preset.is_some_and(|preset| preset.id == "tauri")
        && manifest
            .as_deref()
            .is_some_and(|manifest| presets::cargo_workspace_contains(manifest, "src-tauri"))
    {
        Some("target/release/bundle/macos/*.app")
    } else {
        preset.and_then(|preset| preset.artifact)
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
        artifact,
        binaries: &binaries,
        xcode_scheme: xcode_scheme.as_deref(),
        release: latest_release.as_ref(),
        asset_kind,
    };
    let (mut suggestion, mut notes) = suggest(&facts);
    if let Some(preset) = preset
        && let Source::Github(github) = &suggestion.source
        && let Install::Source(source) = &github.install
    {
        build_candidates.push(BuildCandidate {
            id: format!("preset:{}", preset.id),
            source: BuildCandidateSource::Preset,
            file: if preset.id == "headway-app" {
                "scripts/build-headway.sh"
            } else {
                ""
            }
            .into(),
            line: None,
            build: source.build.clone(),
            workdir: source.workdir.clone(),
            artifact: source.artifact.clone(),
            kind: preset.kind,
            reason: format!(
                "Detected {} source layout; review the preset before saving.",
                preset.label
            ),
            recommended: false,
        });
    }
    build_detection::prioritize(
        &mut build_candidates,
        preset.is_some_and(|preset| preset.id == "headway-app"),
    );
    if let Some(candidate) = build_candidates
        .iter()
        .find(|candidate| candidate.recommended)
    {
        suggestion.kind = candidate.kind;
        if let Source::Github(github) = &mut suggestion.source {
            github.track = Track::Commit { branch: None };
            github.install = Install::Source(SourceInstall {
                build: candidate.build.clone(),
                workdir: candidate.workdir.clone(),
                artifact: candidate.artifact.clone(),
                ..SourceInstall::default()
            });
        }
        notes.retain(|note| {
            !note.starts_with("No known build system") && !note.starts_with("Could not detect")
        });
        notes.push(candidate.reason.clone());
        if latest_release
            .as_ref()
            .is_some_and(|release| release.suggested_asset.is_some())
            && preset.is_none()
        {
            notes.push(
                "A macOS asset is also available: choose Releases to download it instead.".into(),
            );
        }
        if candidate.artifact.is_none() {
            notes.push("The output path is unknown; edit the artifact or use a command that installs the program itself.".into());
        }
    }
    if !build_candidates.is_empty()
        && !build_candidates
            .iter()
            .any(|candidate| candidate.recommended)
    {
        notes.push("Build commands were found, but their install outputs are unknown. Select a candidate and set its artifact path; no build-only command was selected as self-installing.".into());
    }
    notes.extend(discovery.notes);
    if let Source::Github(github) = &mut suggestion.source {
        github.options.repositories = crate::repository_family::choices(&info)?;
    }
    let build_system = preset.map(|preset| preset.id.to_owned()).or_else(|| {
        build_candidates.iter().find_map(|candidate| {
            if candidate.file.ends_with("Cargo.toml") {
                Some("cargo".into())
            } else if candidate.file == "package.json" {
                Some("npm".into())
            } else if candidate.file == "main.go" {
                Some("go".into())
            } else if candidate.file == "Package.swift" {
                Some("swiftpm".into())
            } else if matches!(
                candidate.file.as_str(),
                "Makefile" | "makefile" | "GNUmakefile"
            ) {
                Some("make".into())
            } else {
                None
            }
        })
    });
    Ok(RepoInspection {
        repo: repo.full_name(),
        fork: info.fork,
        parent: info.parent,
        source: info.source,
        description: info.description.filter(|text| !text.trim().is_empty()),
        default_branch: info.default_branch,
        private: info.private,
        archived: info.archived,
        html_url: info.html_url,
        build_system,
        build_candidates,
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

#[cfg(test)]
#[path = "inspect_tests.rs"]
mod tests;
