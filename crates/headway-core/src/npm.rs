//! Globally installed npm packages: versions from `npm view`, the installed one from
//! `npm ls -g`, updates with `npm install -g <package>@<version>`.
//!
//! Everything goes through the user's own `npm`, so `.npmrc` registries, mirrors and
//! tokens apply.

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::check::{Checked, InstalledSync, RECENT, newest_first};
use crate::config::{AppKind, TrackedApp};
use crate::error::{Error, Result};
use crate::events::Stage;
use crate::inspect::PackageLookup;
use crate::probe;
use crate::process::{self, Cmd};
use crate::repo::slugify;
use crate::shell::ShellEnv;
use crate::source::{NpmSource, Source};
use crate::state::{Candidate, CheckResult, UpdateStatus};
use crate::update::{Installed, Job};
use crate::version::Version;

/// Pending versions kept in a check result (the total is still counted).
const PENDING_LIMIT: usize = 100;

#[derive(Debug, Default, Deserialize)]
struct View {
    #[serde(default)]
    versions: Option<OneOrMany>,
    #[serde(default, rename = "dist-tags")]
    dist_tags: BTreeMap<String, String>,
    #[serde(default)]
    time: BTreeMap<String, String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
}

/// `npm view` prints a single version as a string, several as an array.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl View {
    fn versions(&self) -> Vec<&str> {
        match &self.versions {
            Some(OneOrMany::One(version)) => vec![version.as_str()],
            Some(OneOrMany::Many(versions)) => versions.iter().map(String::as_str).collect(),
            None => Vec::new(),
        }
    }
}

async fn view(cancel: &CancellationToken, env: &ShellEnv, package: &str) -> Result<View> {
    let cmd = Cmd::new("npm", env).args([
        "view",
        package,
        "versions",
        "dist-tags",
        "time",
        "description",
        "homepage",
        "--json",
    ]);
    let out = probe::capture(cancel, &cmd).await?;
    if !out.success() {
        if out.stdout.contains("E404") || out.stderr.contains("E404") {
            return Err(Error::RemoteNotFound(format!("npm package `{package}`")));
        }
        return Err(probe::failed(&cmd, &out));
    }
    serde_json::from_str(&out.stdout)
        .map_err(|err| Error::parse(format!("`npm view {package}` output"), &err))
}

#[derive(Debug, Default, Deserialize)]
struct Listing {
    #[serde(default)]
    dependencies: BTreeMap<String, Listed>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Listed {
    #[serde(default)]
    version: Option<String>,
}

/// The globally installed version, `None` when the package isn't installed.
pub async fn installed_version(
    cancel: &CancellationToken,
    env: &ShellEnv,
    package: &str,
) -> Result<Option<String>> {
    let cmd = Cmd::new("npm", env).args(["ls", "--global", "--depth=0", "--json", package]);
    // `npm ls` exits 1 when the package is missing; the JSON still says so.
    let out = probe::capture(cancel, &cmd).await?;
    let listing: Listing = serde_json::from_str(&out.stdout).map_err(|err| {
        if out.success() {
            Error::parse("`npm ls` output", &err)
        } else {
            probe::failed(&cmd, &out)
        }
    })?;
    if listing.error.is_some() || (!out.success() && out.status != Some(1)) {
        return Err(probe::failed(&cmd, &out));
    }
    Ok(listing
        .dependencies
        .get(package)
        .and_then(|listed| listed.version.clone()))
}

fn page(package: &str, version: &str) -> String {
    format!("https://www.npmjs.com/package/{package}/v/{version}")
}

/// Versions on the followed dist-tag's channel, newest first, and the tag's version.
fn candidates(view: &View, source: &NpmSource) -> Result<(Vec<Candidate>, Candidate)> {
    let head_id = view.dist_tags.get(&source.dist_tag).ok_or_else(|| {
        Error::RemoteNotFound(format!(
            "dist-tag `{}` of npm package `{}`",
            source.dist_tag, source.package
        ))
    })?;
    let head = Version::parse(head_id)
        .ok_or_else(|| Error::parse(format!("npm version `{head_id}`"), &"not a version"))?;
    // `latest` is the stable channel; other tags (next, beta …) include pre-releases.
    let include_pre = source.dist_tag != "latest" || head.is_prerelease();
    let candidate = |id: &str, version: &Version| Candidate {
        id: id.to_owned(),
        sha: None,
        title: id.to_owned(),
        body: None,
        author: None,
        date: view
            .time
            .get(id)
            .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
            .map(|time| time.with_timezone(&Utc)),
        url: Some(page(&source.package, id)),
        prerelease: version.is_prerelease(),
    };
    let mut list: Vec<Candidate> = view
        .versions()
        .into_iter()
        .filter_map(|id| {
            let version = Version::parse(id)?;
            let on_channel = version <= head && (include_pre || !version.is_prerelease());
            on_channel.then(|| candidate(id, &version))
        })
        .collect();
    list.sort_by(newest_first);
    let head_candidate = list
        .iter()
        .find(|item| item.id == *head_id)
        .cloned()
        .unwrap_or_else(|| candidate(head_id, &head));
    Ok((list, head_candidate))
}

pub async fn check(
    cancel: &CancellationToken,
    env: &ShellEnv,
    source: &NpmSource,
) -> Result<Checked> {
    let view = view(cancel, env, &source.package).await?;
    let installed = installed_version(cancel, env, &source.package).await?;
    let (list, latest) = candidates(&view, source)?;
    let result = |status, pending: Vec<Candidate>, total: usize| CheckResult {
        checked_at: Utc::now(),
        status,
        branch: None,
        latest: Some(latest.clone()),
        pending_total: u32::try_from(total).unwrap_or(u32::MAX),
        pending,
    };
    let result = match installed.as_deref().map(|id| (id, Version::parse(id))) {
        None => result(
            UpdateStatus::NotInstalled,
            list.into_iter().take(RECENT).collect(),
            0,
        ),
        Some((_, current)) => {
            let newer: Vec<Candidate> = list
                .into_iter()
                .filter(|item| match (&current, Version::parse(&item.id)) {
                    (Some(current), Some(version)) => version > *current,
                    _ => false,
                })
                .collect();
            let total = newer.len();
            let status = if total == 0 {
                UpdateStatus::UpToDate
            } else {
                UpdateStatus::UpdateAvailable
            };
            // Keep the true next version even when the newest window is truncated.
            let next = (total > PENDING_LIMIT)
                .then(|| newer.last().cloned())
                .flatten();
            let keep = PENDING_LIMIT.saturating_sub(usize::from(next.is_some()));
            let pending = newer.into_iter().take(keep).chain(next).collect();
            result(status, pending, total)
        }
    };
    Ok(Checked {
        result,
        installed: InstalledSync::Detected(installed),
    })
}

/// The candidate for an explicit version (`--to 1.2.3`).
pub async fn version(
    cancel: &CancellationToken,
    env: &ShellEnv,
    source: &NpmSource,
    id: &str,
) -> Result<Candidate> {
    let view = view(cancel, env, &source.package).await?;
    if !view.versions().contains(&id) {
        return Err(Error::RemoteNotFound(format!(
            "version `{id}` of npm package `{}`",
            source.package
        )));
    }
    let prerelease = Version::parse(id).is_some_and(|version| version.is_prerelease());
    Ok(Candidate {
        id: id.to_owned(),
        sha: None,
        title: id.to_owned(),
        body: None,
        author: None,
        date: None,
        url: Some(page(&source.package, id)),
        prerelease,
    })
}

pub fn install_cmd(env: &ShellEnv, source: &NpmSource, version: &str) -> Cmd {
    Cmd::new("npm", env).args([
        "install".to_owned(),
        "--global".to_owned(),
        format!("{}@{version}", source.package),
    ])
}

pub async fn install(job: &Job<'_>, source: &NpmSource, target: &Candidate) -> Result<Installed> {
    let cancel = job.cancel;
    let cmd = install_cmd(job.env, source, &target.id);
    job.log
        .stage(Stage::Install, format!("Running `{}`", cmd.display()));
    process::run(&cmd, job.cancel, &mut job.output()).await?;
    let version = installed_version(cancel, job.env, &source.package)
        .await?
        .ok_or_else(|| {
            Error::Config(format!(
                "npm finished but {} is not installed",
                source.package
            ))
        })?;
    if version != target.id {
        return Err(Error::Config(format!(
            "npm finished but {} is at {version}, expected {}",
            source.package, target.id
        )));
    }
    Ok(Installed {
        sha: None,
        version: Some(version),
        path: None,
    })
}

pub async fn lookup(
    cancel: &CancellationToken,
    env: &ShellEnv,
    package: &str,
) -> Result<PackageLookup> {
    let view = view(cancel, env, package).await?;
    let installed = installed_version(cancel, env, package).await?;
    let latest = view.dist_tags.get("latest").cloned();
    let short = package.rsplit('/').next().unwrap_or(package);
    let mut notes = vec![format!(
        "npm package {package}: latest {}, {} versions published.",
        latest.as_deref().unwrap_or("unknown"),
        view.versions().len()
    )];
    notes.push(installed.as_ref().map_or_else(
        || "Not installed globally yet: `headway update` installs it.".to_owned(),
        |version| format!("Installed globally: {version}."),
    ));
    Ok(PackageLookup {
        source: "npm",
        package: package.to_owned(),
        description: view
            .description
            .clone()
            .filter(|text| !text.trim().is_empty()),
        homepage: view.homepage.clone(),
        latest,
        installed,
        suggestion: TrackedApp {
            id: slugify(short),
            name: short.to_owned(),
            kind: AppKind::Cli,
            auto_update: false,
            update_policy: crate::config::UpdatePolicy::default(),
            source: Source::Npm(NpmSource {
                package: package.to_owned(),
                dist_tag: "latest".to_owned(),
            }),
        },
        notes,
    })
}

#[cfg(test)]
#[path = "npm_tests.rs"]
mod tests;
