//! Read-only evidence about an explicitly located GitHub payload, never its revision.
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use crate::check::{Checked, Probe};
use crate::config::{AppKind, Install, Track, TrackedApp};
use crate::error::{Error, Result};
use crate::state::{InstalledRef, UpdateStatus};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Unknown,
    Present,
    Missing,
    Invalid,
}

pub(super) fn location<'a>(
    app: &'a TrackedApp,
    installed: Option<&'a InstalledRef>,
) -> Option<&'a str> {
    let github = app.github()?;
    let configured = match &github.install {
        Install::Source(source) => source.dest.as_deref(),
        Install::Asset(asset) => asset.dest.as_deref(),
    };
    // Destinations are literal paths, not shell expansions or asset globs.
    let literal = |path: &&str| !path.contains(['*', '?']);
    configured.filter(literal).or_else(|| {
        installed
            .and_then(|installed| installed.path.as_deref())
            .filter(literal)
    })
}

pub(super) fn observe(app: &TrackedApp, installed: Option<&InstalledRef>) -> Result<Presence> {
    let Some(path) = location(app, installed) else {
        return Ok(Presence::Unknown);
    };
    let path = crate::paths::expand_tilde(path);
    let Some(metadata) = metadata(&path)? else {
        return Ok(Presence::Missing);
    };
    let present = match app.kind {
        AppKind::Cli => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        AppKind::App => {
            metadata.is_dir()
                && crate::install::is_bundle(&path)
                && metadata_file(&path.join("Contents/Info.plist"))?
        }
    };
    Ok(if present {
        Presence::Present
    } else {
        Presence::Invalid
    })
}

fn metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
    metadata_result(path, std::fs::metadata(path))
}

fn metadata_result(
    path: &Path,
    result: std::io::Result<std::fs::Metadata>,
) -> Result<Option<std::fs::Metadata>> {
    match result {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io_at("inspect installed payload", path, error)),
    }
}

fn metadata_file(path: &Path) -> Result<bool> {
    Ok(metadata(path)?.is_some_and(|metadata| metadata.is_file()))
}

pub(super) fn has_baseline(app: &TrackedApp, installed: Option<&InstalledRef>) -> bool {
    if !same_location(app, installed) {
        return false;
    }
    match app.github().map(|github| &github.track) {
        Some(Track::Commit { .. }) => installed.is_some_and(|installed| installed.sha.is_some()),
        Some(Track::Release { .. } | Track::Tag { .. }) | None => installed.is_some(),
    }
}

fn same_location(app: &TrackedApp, installed: Option<&InstalledRef>) -> bool {
    match (
        location(app, installed),
        installed.and_then(|value| value.path.as_deref()),
    ) {
        (Some(path), Some(recorded)) => {
            crate::paths::expand_tilde(path) == crate::paths::expand_tilde(recorded)
        }
        _ => true,
    }
}

pub(super) fn retained(
    app: &TrackedApp,
    installed: Option<InstalledRef>,
) -> Result<Option<InstalledRef>> {
    match observe(app, installed.as_ref())? {
        Presence::Missing | Presence::Invalid => Ok(None),
        Presence::Present | Presence::Unknown => {
            Ok(installed.filter(|value| same_location(app, Some(value))))
        }
    }
}

pub(super) async fn check(
    probe: &Probe<'_>,
    app: &TrackedApp,
    installed: Option<&InstalledRef>,
) -> Result<Checked> {
    let presence = observe(app, installed)?;
    let baseline = match presence {
        Presence::Missing | Presence::Invalid => None,
        Presence::Present | Presence::Unknown => {
            installed.filter(|value| same_location(app, Some(value)))
        }
    };
    let mut checked = crate::check::check(probe, app, baseline).await?;
    if matches!(presence, Presence::Present | Presence::Invalid) && !has_baseline(app, baseline) {
        checked.result.status = UpdateStatus::Unknown;
        checked.result.pending_total = 0;
    }
    Ok(checked)
}

#[cfg(test)]
#[path = "presence_edge_tests.rs"]
mod tests;
