use crate::config::{AppKind, AssetInstall, Install, SourceInstall, Track, TrackedApp};
use crate::inspect::ReleaseSummary;
use crate::presets::{self, Preset};
use crate::repo::{RepoId, slugify};
use crate::source::{GithubSource, Source};

pub(crate) struct Facts<'a> {
    pub repo: &'a RepoId,
    pub default_branch: &'a str,
    pub preset: Option<&'static Preset>,
    pub artifact: Option<&'static str>,
    pub binaries: &'a [String],
    pub xcode_scheme: Option<&'a str>,
    pub release: Option<&'a ReleaseSummary>,
    pub asset_kind: AppKind,
}

pub(crate) fn suggest(facts: &Facts<'_>) -> (TrackedApp, Vec<String>) {
    let repo = facts.repo;
    let bin = facts
        .binaries
        .first()
        .cloned()
        .unwrap_or_else(|| repo.name.clone());
    let base = |kind, track, install| TrackedApp {
        id: slugify(&repo.name),
        name: match facts.preset.map(|preset| preset.id) {
            Some("headway-app") => "Headway".to_owned(),
            _ => repo.name.clone(),
        },
        kind,
        auto_update: false,
        update_policy: crate::policy::UpdatePolicy::default(),
        source: Source::Github(GithubSource {
            options: Box::default(),
            repo: repo.full_name(),
            git_url: None,
            track,
            install,
        }),
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
            // Path templates are substituted raw (unlike shell-quoted build commands), so
            // validate the rendered artifact before offering it for saving.
            let artifact = facts.artifact.and_then(|template| {
                let rendered = super::render_artifact(template, &bin, name);
                if rendered.is_none() {
                    notes.push(
                        "The detected artifact path does not stay inside the checkout; set the artifact explicitly in the config."
                            .to_owned(),
                    );
                }
                rendered
            });
            base(
                preset.kind,
                Track::Commit { branch: None },
                Install::Source(SourceInstall {
                    storage: crate::source_storage::SourceStorage::default(),
                    build: presets::render_command(preset.build, &bin, name),
                    workdir: None,
                    artifact,
                    dest: None,
                    verify_command: None,
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
                    verify_command: None,
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
