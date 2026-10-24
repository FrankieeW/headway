//! Installing one target (a commit or a release) for an app: the build or
//! download pipeline itself. Recording the result is up to [`crate::service`].

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::archive::{self, ArchiveKind};
use crate::asset;
use crate::config::{AppKind, Install, Settings, TrackedApp};
use crate::error::{Error, Result};
use crate::github::{Asset, GitHub};
use crate::install;
use crate::paths::{Paths, display_path};
use crate::shell::ShellEnv;
use crate::source::Source;
use crate::state::{Candidate, InstalledRef};

mod job;
mod log;
pub use log::{LOG_KEEP, RunLog, latest_log};
#[cfg(test)]
use log::{log_files, prune_logs};

/// What to install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// The newest commit on the branch / the newest release or tag.
    Latest,
    /// The candidate right after the installed one (one commit or version at a time).
    Next,
    /// A specific commit SHA, branch or tag.
    Ref(String),
}

impl Target {
    /// `latest`, `next`, or anything else as a ref.
    pub fn parse(input: &str) -> Self {
        match input.trim() {
            "latest" | "" => Self::Latest,
            "next" => Self::Next,
            other => Self::Ref(other.to_owned()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Latest => "the latest version".to_owned(),
            Self::Next => "the next version".to_owned(),
            Self::Ref(reference) => format!("`{reference}`"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetChoice {
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// What `update --dry-run` prints.
#[derive(Debug, Clone, Serialize)]
pub struct UpdatePlan {
    pub app_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<InstalledRef>,
    pub to: Candidate,
    /// The target is what is installed already; a real run would do nothing.
    pub up_to_date: bool,
    /// `--to` the installed version: a real run builds/downloads it again.
    pub reinstall: bool,
    pub method: &'static str,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetChoice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateOutcome {
    pub app_id: String,
    /// `false` when the target was already installed.
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<InstalledRef>,
    pub to: InstalledRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    pub duration_ms: u64,
}

/// Result of a successful job.
#[derive(Debug, Clone)]
pub struct Installed {
    pub sha: Option<String>,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

/// Everything one update run needs.
pub struct Job<'a> {
    pub app: &'a TrackedApp,
    pub settings: &'a Settings,
    pub paths: &'a Paths,
    pub gh: &'a GitHub,
    pub env: &'a ShellEnv,
    /// Used for git over HTTPS; never handed to build commands.
    pub token: Option<&'a str>,
    pub cancel: &'a CancellationToken,
    pub log: &'a RunLog<'a>,
}

/// Resolves `artifact` under `workdir`; `*`/`?` in the last segment pick the newest match.
pub fn resolve_artifact(workdir: &Path, artifact: &str) -> Result<PathBuf> {
    let path = workdir.join(artifact);
    let missing = |detail: String| {
        Error::Config(format!(
            "the build finished but install.artifact `{artifact}` {detail} (in {})",
            display_path(workdir)
        ))
    };
    let name = file_name(&path)?;
    if !name.contains(['*', '?']) {
        return if path.exists() {
            Ok(path)
        } else {
            Err(missing("does not exist".into()))
        };
    }
    let parent = path.parent().unwrap_or(workdir);
    let pattern = asset::glob_regex(&name)?;
    let newest = fs::read_dir(parent)
        .map_err(|_| missing("has no parent directory".into()))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| pattern.is_match(&entry.file_name().to_string_lossy()))
        .max_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    newest
        .map(|entry| entry.path())
        .ok_or_else(|| missing("matches nothing".into()))
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::InvalidInput(format!("{} has no file name", path.display())))
}

/// First 7 characters of a SHA, other refs unchanged.
pub fn short(reference: &str) -> String {
    let is_sha = reference.len() >= 12 && reference.chars().all(|c| c.is_ascii_hexdigit());
    if is_sha {
        reference.chars().take(7).collect()
    } else {
        reference.to_owned()
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024 * 10 && unit < UNITS.len() - 1 {
        value /= 1024;
        unit += 1;
    }
    format!("{value} {}", UNITS[unit])
}

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
            let dir = paths.source_dir(&app.id);
            let url = github.clone_url().unwrap_or_default();
            let fetch = if dir.join(".git").is_dir() {
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
            vec![
                fetch,
                checkout,
                format!("run in {}: {}", display_path(&workdir), source.build),
                install,
            ]
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
            if asset.digest.is_some() {
                steps.push("verify the SHA-256 digest published by GitHub".to_owned());
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

#[cfg(test)]
#[path = "../update_tests.rs"]
mod tests;
