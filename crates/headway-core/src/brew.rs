//! Homebrew formulae and casks: `brew info --json=v2` knows the current and the
//! installed version, `brew upgrade` (or `brew install`) updates.
//!
//! Homebrew only installs its current version, so there is no `--next` or `--to`.

use tokio_util::sync::CancellationToken;

use chrono::Utc;
use serde::Deserialize;

use crate::check::{Checked, InstalledSync};
use crate::config::{AppKind, TrackedApp};
use crate::error::{Error, Result};
use crate::events::Stage;
use crate::inspect::PackageLookup;
use crate::probe;
use crate::process::{self, Cmd};
use crate::repo::slugify;
use crate::shell::ShellEnv;
use crate::source::{BrewSource, Source};
use crate::state::{Candidate, CheckResult, UpdateStatus};
use crate::update::{Installed, Job};

#[derive(Debug, Default, Deserialize)]
struct Info {
    #[serde(default)]
    formulae: Vec<Formula>,
    #[serde(default)]
    casks: Vec<Cask>,
}

#[derive(Debug, Deserialize)]
struct Formula {
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    versions: FormulaVersions,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    installed: Vec<Keg>,
    #[serde(default)]
    linked_keg: Option<String>,
    #[serde(default)]
    outdated: bool,
    #[serde(default)]
    pinned: bool,
}

#[derive(Debug, Deserialize)]
struct FormulaVersions {
    #[serde(default)]
    stable: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Keg {
    version: String,
}

#[derive(Debug, Deserialize)]
struct Cask {
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    version: String,
    #[serde(default)]
    installed: Option<String>,
    #[serde(default)]
    outdated: bool,
}

/// What Homebrew knows about one formula or cask.
#[derive(Debug, Clone)]
struct Status {
    latest: String,
    installed: Option<String>,
    outdated: bool,
    pinned: bool,
    title: Option<String>,
    desc: Option<String>,
    homepage: Option<String>,
}

fn noun(cask: bool) -> &'static str {
    if cask { "cask" } else { "formula" }
}

async fn status(cancel: &CancellationToken, env: &ShellEnv, source: &BrewSource) -> Result<Status> {
    let flag = if source.cask { "--cask" } else { "--formula" };
    let cmd = Cmd::new("brew", env)
        .args(["info", "--json=v2", flag, &source.package])
        .env("HOMEBREW_NO_ENV_HINTS", "1");
    let out = probe::capture(cancel, &cmd).await?;
    if !out.success() {
        if out.stderr.contains("No available") || out.stderr.contains("No formulae or casks found")
        {
            return Err(Error::RemoteNotFound(format!(
                "Homebrew {} `{}`",
                noun(source.cask),
                source.package
            )));
        }
        return Err(probe::failed(&cmd, &out));
    }
    let info: Info = serde_json::from_str(&out.stdout)
        .map_err(|err| Error::parse(format!("`brew info {}` output", source.package), &err))?;
    let missing = || {
        Error::RemoteNotFound(format!(
            "Homebrew {} `{}`",
            noun(source.cask),
            source.package
        ))
    };
    if source.cask {
        let cask = info.casks.into_iter().next().ok_or_else(missing)?;
        Ok(Status {
            latest: cask.version,
            installed: cask.installed,
            outdated: cask.outdated,
            pinned: false,
            title: cask.name.into_iter().next(),
            desc: cask.desc,
            homepage: cask.homepage,
        })
    } else {
        let formula = info.formulae.into_iter().next().ok_or_else(missing)?;
        let installed = installed_formula_version(&formula);
        let stable = formula.versions.stable.ok_or_else(missing)?;
        // Rebuilds of the same version are installed as `1.2.3_1`.
        let latest = if formula.revision > 0 {
            format!("{stable}_{}", formula.revision)
        } else {
            stable
        };
        Ok(Status {
            latest,
            installed,
            outdated: formula.outdated,
            pinned: formula.pinned,
            title: None,
            desc: formula.desc,
            homepage: formula.homepage,
        })
    }
}

fn installed_formula_version(formula: &Formula) -> Option<String> {
    if let Some(linked) = &formula.linked_keg
        && formula.installed.iter().any(|keg| &keg.version == linked)
    {
        return Some(linked.clone());
    }
    formula
        .installed
        .iter()
        .max_by(|a, b| {
            match (
                crate::version::Version::parse(&a.version),
                crate::version::Version::parse(&b.version),
            ) {
                (Some(a), Some(b)) => a.cmp(&b),
                _ => a.version.cmp(&b.version),
            }
        })
        .map(|keg| keg.version.clone())
}

pub async fn check(
    cancel: &CancellationToken,
    env: &ShellEnv,
    source: &BrewSource,
) -> Result<Checked> {
    let status = status(cancel, env, source).await?;
    let latest = Candidate {
        id: status.latest.clone(),
        sha: None,
        title: status.latest.clone(),
        body: status
            .pinned
            .then(|| "Pinned: `brew upgrade` leaves it alone until `brew unpin`.".to_owned()),
        author: None,
        date: None,
        url: Source::Brew(source.clone()).web_url(),
        prerelease: false,
    };
    let (update_status, pending) = match (&status.installed, status.outdated) {
        (None, _) => (UpdateStatus::NotInstalled, vec![latest.clone()]),
        (Some(_), true) => (UpdateStatus::UpdateAvailable, vec![latest.clone()]),
        (Some(_), false) => (UpdateStatus::UpToDate, Vec::new()),
    };
    let pending_total = u32::from(update_status == UpdateStatus::UpdateAvailable);
    Ok(Checked {
        result: CheckResult {
            checked_at: Utc::now(),
            status: update_status,
            branch: None,
            latest: Some(latest),
            pending,
            pending_total,
        },
        installed: InstalledSync::Detected(status.installed),
    })
}

/// `brew upgrade` when installed, `brew install` otherwise; `--cask` for casks.
pub fn update_cmd(env: &ShellEnv, source: &BrewSource, installed: bool) -> Cmd {
    let verb = if installed { "upgrade" } else { "install" };
    let cmd = Cmd::new("brew", env).arg(verb);
    let cmd = cmd.arg(if source.cask { "--cask" } else { "--formula" });
    cmd.arg(source.package.clone())
        .env("HOMEBREW_NO_ENV_HINTS", "1")
}

pub async fn install(job: &Job<'_>, source: &BrewSource) -> Result<Installed> {
    let cancel = job.cancel;
    let before = status(cancel, job.env, source).await?;
    if before.pinned {
        return Err(Error::InvalidInput(format!(
            "{} is pinned; use `brew unpin {}` before updating",
            source.package, source.package
        )));
    }
    let cmd = update_cmd(job.env, source, before.installed.is_some());
    job.log
        .stage(Stage::Install, format!("Running `{}`", cmd.display()));
    process::run(&cmd, job.cancel, &mut job.output()).await?;
    let after = status(cancel, job.env, source).await?;
    let installed = after.installed.ok_or_else(|| {
        Error::Config(format!(
            "brew finished but {} is not installed",
            source.package
        ))
    })?;
    Ok(Installed {
        sha: None,
        version: Some(installed),
        path: None,
    })
}

pub async fn lookup(
    cancel: &CancellationToken,
    env: &ShellEnv,
    package: &str,
    cask: bool,
) -> Result<PackageLookup> {
    let source = BrewSource {
        install_path: None,
        package: package.to_owned(),
        cask,
    };
    let status = status(cancel, env, &source).await?;
    let short = package.rsplit('/').next().unwrap_or(package);
    let mut notes = vec![format!(
        "Homebrew {} {package}: current version {}.",
        noun(cask),
        status.latest
    )];
    notes.push(match (&status.installed, status.outdated) {
        (None, _) => "Not installed yet: `headway update` installs it with brew.".to_owned(),
        (Some(version), true) => format!("Installed {version}; an upgrade is available."),
        (Some(version), false) => format!("Installed {version}, up to date."),
    });
    Ok(PackageLookup {
        source: "brew",
        package: package.to_owned(),
        description: status.desc.clone(),
        homepage: status.homepage.clone(),
        latest: Some(status.latest.clone()),
        installed: status.installed.clone(),
        suggestion: TrackedApp {
            id: slugify(short),
            name: status.title.clone().unwrap_or_else(|| short.to_owned()),
            kind: if cask { AppKind::App } else { AppKind::Cli },
            auto_update: false,
            update_policy: crate::config::UpdatePolicy::default(),
            source: Source::Brew(source),
        },
        notes,
    })
}

#[cfg(test)]
#[path = "brew_tests.rs"]
mod tests;
