use super::{human_bytes, short};
use crate::archive::{self, ArchiveKind};
use crate::config::{AppKind, Install, Settings, TrackedApp};
use crate::github::Asset;
use crate::install;
use crate::paths::{Paths, display_path};
use crate::source::Source;
use crate::state::Candidate;
use std::path::Path;

/// Human-readable steps an update would take (for `--dry-run`).
pub fn plan_steps(
    app: &TrackedApp,
    settings: &Settings,
    paths: &Paths,
    target: &Candidate,
    asset: Option<&Asset>,
) -> Vec<String> {
    let github = match &app.source {
        Source::Github(github) => github,
        Source::Npm(source) => {
            return vec![format!(
                "npm install --global {}@{}",
                source.package, target.id
            )];
        }
        Source::Brew(source) => {
            return vec![format!(
                "brew install/upgrade {}{}",
                if source.cask { "--cask " } else { "--formula " },
                source.package
            )];
        }
        Source::Script(source) => return vec![source.update_command.clone()],
    };
    match &github.install {
        Install::Source(source) => {
            let location =
                match crate::source_storage::Location::new(&source.storage, paths, &app.id) {
                    Ok(location) => location,
                    Err(error) => return vec![format!("invalid source storage: {error}")],
                };
            let dir = if source.storage.keep {
                location.path
            } else {
                location.base.join("<temporary-checkout>")
            };
            let url = github.clone_url().unwrap_or_default();
            let fetch = if source.storage.keep && dir.join(".git").is_dir() {
                format!("fetch {url} in {}", display_path(&dir))
            } else {
                format!("clone {url} into {}", display_path(&dir))
            };
            let checkout = target.sha.as_deref().map_or_else(
                || format!("check out tag {}", target.id),
                |sha| format!("check out {}", short(sha)),
            );
            let workdir = source
                .workdir
                .as_deref()
                .map_or_else(|| dir.clone(), |sub| dir.join(sub));
            let install = match &source.artifact {
                Some(artifact) => {
                    let name = Path::new(artifact)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let dest =
                        install::resolve_dest(source.dest.as_deref(), app.kind, &name, settings);
                    format!("install {artifact} → {}", display_path(&dest))
                }
                None => "nothing to copy: the build command installs the program itself".to_owned(),
            };
            let mut steps = vec![
                fetch,
                checkout,
                format!("run in {}: {}", display_path(&workdir), source.build),
                install,
            ];
            steps.push(if source.storage.keep {
                "retain the managed source checkout and build cache".into()
            } else {
                "remove the temporary source checkout after success, failure, or cancellation"
                    .into()
            });
            steps
        }
        Install::Asset(spec) => {
            let Some(asset) = asset else {
                return vec!["no release asset matches; the update would fail".to_owned()];
            };
            let mut steps = vec![format!(
                "download {} ({})",
                asset.name,
                human_bytes(asset.size)
            )];
            if asset
                .digest
                .as_deref()
                .is_some_and(|digest| digest.starts_with("sha256:"))
            {
                steps.push("verify the SHA-256 digest published by GitHub (integrity, not publisher authentication)".to_owned());
            } else {
                steps.push("WARNING: GitHub published no supported SHA-256 digest; download will proceed without published-checksum verification and report the computed SHA-256".to_owned());
            }
            let repo_name = app.repo_id().map(|repo| repo.name).unwrap_or_default();
            let raw = archive::detect(&asset.name) == ArchiveKind::Raw;
            let step = match (app.kind, spec.binary.as_deref()) {
                (AppKind::Cli, None) if raw => {
                    let dest =
                        install::resolve_dest(spec.dest.as_deref(), app.kind, &repo_name, settings);
                    format!("install it as {}", display_path(&dest))
                }
                (AppKind::Cli, None) => format!(
                    "unpack it and install the executable named `{repo_name}` (or the only one inside) into {}",
                    spec.dest.as_deref().unwrap_or(&settings.bin_dir)
                ),
                (AppKind::App, None) => format!(
                    "unpack it and install the .app inside into {}",
                    spec.dest.as_deref().unwrap_or(&settings.apps_dir)
                ),
                (kind, Some(name)) => {
                    let payload =
                        if kind == AppKind::App && !name.to_ascii_lowercase().ends_with(".app") {
                            format!("{name}.app")
                        } else {
                            name.to_owned()
                        };
                    let dest =
                        install::resolve_dest(spec.dest.as_deref(), kind, &payload, settings);
                    format!("unpack it and install {payload} → {}", display_path(&dest))
                }
            };
            steps.push(step);
            steps
        }
    }
}
