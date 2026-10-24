//! Turning `add`/`edit` flags into a config, on top of a base (the inspection's
//! suggestion, or the current config). Flags that don't fit the chosen mode are
//! rejected instead of being silently ignored.

use crate::args::UpdateModeArg;
use headway_core::config::{AppKind, AssetInstall, Install, SourceInstall, Track, TrackedApp};
use headway_core::{AutoUpdateMode, UpdatePolicy, presets};
use headway_core::{Error, GithubSource, Result, Source};

use crate::args::{AppFields, KindArg, MethodArg, TrackArg};

/// `None` keeps the base value, `Some("")` clears it, anything else sets it.
pub(crate) fn optional(flag: Option<&String>, base: Option<String>) -> Option<String> {
    match flag {
        None => base,
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(value.trim().to_owned()),
    }
}

fn misuse(flag: &str, needs: &str) -> Error {
    Error::InvalidInput(format!("{flag} only applies to {needs}"))
}

pub fn apply(base: TrackedApp, fields: &AppFields) -> Result<TrackedApp> {
    let kind = match fields.kind {
        Some(KindArg::Cli) => AppKind::Cli,
        Some(KindArg::App) => AppKind::App,
        None => match (&base.source, fields.cask) {
            (Source::Brew(_), Some(true)) => AppKind::App,
            (Source::Brew(_), Some(false)) => AppKind::Cli,
            _ => fields
                .preset
                .as_deref()
                .and_then(presets::find)
                .map_or(base.kind, |preset| preset.kind),
        },
    };
    let name = fields.name.clone().unwrap_or(base.name);
    crate::source_fields::validate(&base.source, fields)?;
    let source = match base.source {
        Source::Github(github) => {
            let track = apply_track(&github.track, fields)?;
            let install = apply_install(github.install, &track, fields, &github.repo, &name)?;
            Source::Github(GithubSource {
                git_url: optional(fields.git_url.as_ref(), github.git_url),
                track,
                install,
                ..github
            })
        }
        source => crate::source_fields::apply(source, fields),
    };
    Ok(TrackedApp {
        name,
        kind,
        auto_update: fields.auto_update.unwrap_or(base.auto_update),
        update_policy: UpdatePolicy {
            mode: match fields.update_mode {
                Some(UpdateModeArg::Notify) => AutoUpdateMode::Notify,
                Some(UpdateModeArg::Next) => AutoUpdateMode::Next,
                Some(UpdateModeArg::Latest) => AutoUpdateMode::Latest,
                None => base.update_policy.mode,
            },
            max_steps: fields
                .max_update_steps
                .unwrap_or(base.update_policy.max_steps),
            pause_on_failure: fields
                .pause_on_failure
                .unwrap_or(base.update_policy.pause_on_failure),
        },
        source,
        ..base
    })
}

fn apply_track(base: &Track, fields: &AppFields) -> Result<Track> {
    let (base_prerelease, base_filter) = match base {
        Track::Release {
            prerelease,
            tag_filter,
        }
        | Track::Tag {
            prerelease,
            tag_filter,
        } => (*prerelease, tag_filter.clone()),
        Track::Commit { .. } => (false, None),
    };
    let base_branch = match base {
        Track::Commit { branch } => branch.clone(),
        Track::Release { .. } | Track::Tag { .. } => None,
    };
    let mode = fields.track.unwrap_or(match base {
        Track::Commit { .. } => TrackArg::Commit,
        Track::Release { .. } => TrackArg::Release,
        Track::Tag { .. } => TrackArg::Tag,
    });
    match mode {
        TrackArg::Commit => {
            if fields.prerelease.is_some() {
                return Err(misuse("--prerelease", "--track release|tag"));
            }
            if fields.tag_filter.is_some() {
                return Err(misuse("--tag-filter", "--track release|tag"));
            }
            Ok(Track::Commit {
                branch: optional(fields.branch.as_ref(), base_branch),
            })
        }
        TrackArg::Release | TrackArg::Tag => {
            if fields.branch.is_some() {
                return Err(misuse("--branch", "--track commit"));
            }
            let prerelease = fields.prerelease.unwrap_or(base_prerelease);
            let tag_filter = optional(fields.tag_filter.as_ref(), base_filter);
            Ok(if mode == TrackArg::Release {
                Track::Release {
                    prerelease,
                    tag_filter,
                }
            } else {
                Track::Tag {
                    prerelease,
                    tag_filter,
                }
            })
        }
    }
}

fn apply_install(
    base: Install,
    track: &Track,
    fields: &AppFields,
    repo: &str,
    name: &str,
) -> Result<Install> {
    let method = fields.method.unwrap_or(match (&base, track) {
        // Asset installs only exist for releases; leaving release mode means building.
        (Install::Asset(_), Track::Release { .. }) => MethodArg::Asset,
        _ => MethodArg::Source,
    });
    match method {
        MethodArg::Source => {
            for (flag, value) in [("--pattern", &fields.pattern), ("--binary", &fields.binary)] {
                if value.is_some() && fields.preset.is_none() {
                    return Err(misuse(
                        flag,
                        "--method asset (or --preset, where --binary fills {bin})",
                    ));
                }
            }
            let base = match base {
                Install::Source(source) => source,
                Install::Asset(_) => SourceInstall::default(),
            };
            let base = match &fields.preset {
                Some(id) => preset_source(id, fields.binary.as_deref(), repo, name, base)?,
                None => base,
            };
            Ok(Install::Source(SourceInstall {
                build: fields.build.clone().unwrap_or(base.build),
                workdir: optional(fields.workdir.as_ref(), base.workdir),
                artifact: optional(fields.artifact.as_ref(), base.artifact),
                dest: optional(fields.dest.as_ref(), base.dest),
                verify_command: optional(fields.verify_command.as_ref(), base.verify_command),
            }))
        }
        MethodArg::Asset => {
            for (flag, value) in [
                ("--build", &fields.build),
                ("--workdir", &fields.workdir),
                ("--artifact", &fields.artifact),
                ("--preset", &fields.preset),
            ] {
                if value.is_some() {
                    return Err(misuse(flag, "--method source"));
                }
            }
            let base = match base {
                Install::Asset(asset) => asset,
                Install::Source(_) => AssetInstall::default(),
            };
            Ok(Install::Asset(AssetInstall {
                pattern: optional(fields.pattern.as_ref(), base.pattern),
                binary: optional(fields.binary.as_ref(), base.binary),
                dest: optional(fields.dest.as_ref(), base.dest),
                verify_command: optional(fields.verify_command.as_ref(), base.verify_command),
            }))
        }
    }
}

fn preset_source(
    id: &str,
    binary: Option<&str>,
    repo: &str,
    name: &str,
    base: SourceInstall,
) -> Result<SourceInstall> {
    let preset = presets::find(id).ok_or_else(|| {
        let known: Vec<&str> = presets::PRESETS.iter().map(|preset| preset.id).collect();
        Error::InvalidInput(format!(
            "unknown preset `{id}`; known: {}",
            known.join(", ")
        ))
    })?;
    let bin = binary.map_or_else(
        || repo.rsplit('/').next().unwrap_or(name).to_owned(),
        str::to_owned,
    );
    Ok(SourceInstall {
        build: presets::render_command(preset.build, &bin, name),
        artifact: preset
            .artifact
            .map(|artifact| presets::render(artifact, &bin, name)),
        ..base
    })
}

#[cfg(test)]
#[path = "fields_tests.rs"]
mod tests;
