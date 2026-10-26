//! Turning `add`/`edit` flags into a config, on top of a base (the inspection's
//! suggestion, or the current config). Flags for another source are rejected.
//! A GitHub app can keep both a source build and a release download; `--method`
//! chooses the default.

use crate::args::UpdateModeArg;
use headway_core::config::{AppKind, AssetInstall, Install, SourceInstall, Track, TrackedApp};
use headway_core::{AutoUpdateMode, GithubOptions, UpdatePolicy, presets};
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
    // A preset fills the source build; it picks the payload kind only when that build is the default.
    let preset_kind = fields
        .preset
        .as_deref()
        .and_then(presets::find)
        .map(|preset| preset.kind)
        .filter(|_| {
            base.github()
                .is_some_and(|github| install_method(&github.install, fields) == MethodArg::Source)
        });
    let kind = match fields.kind {
        Some(KindArg::Cli) => AppKind::Cli,
        Some(KindArg::App) => AppKind::App,
        None => match (&base.source, fields.cask) {
            (Source::Brew(_), Some(true)) => AppKind::App,
            (Source::Brew(_), Some(false)) => AppKind::Cli,
            _ => preset_kind.unwrap_or(base.kind),
        },
    };
    let name = fields.name.clone().unwrap_or(base.name);
    crate::source_fields::validate(&base.source, fields)?;
    let source = match base.source {
        Source::Github(github) => {
            let (track, install, options) = github_recipes(&github, fields, &name)?;
            Source::Github(GithubSource {
                options,
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

fn install_method(base: &Install, fields: &AppFields) -> MethodArg {
    fields.method.unwrap_or(match base {
        Install::Source(_) => MethodArg::Source,
        Install::Asset(_) => MethodArg::Asset,
    })
}

/// Saves the source-build recipe and the release-download recipe together.
///
/// `--method` chooses the default used by an ordinary update. Flags for the other
/// recipe stay on that recipe instead of being rejected as the wrong mode.
fn github_recipes(
    github: &GithubSource,
    fields: &AppFields,
    name: &str,
) -> Result<(Track, Install, Box<GithubOptions>)> {
    let track = apply_track(&github.track, fields)?;
    let mut options = github.options.clone();
    let method = install_method(&github.install, fields);
    let previous_dest = match &github.install {
        Install::Source(source) => source.dest.clone(),
        Install::Asset(asset) => asset.dest.clone(),
    };
    let source_flags = fields.build.is_some()
        || fields.workdir.is_some()
        || fields.artifact.is_some()
        || fields.preset.is_some()
        || fields.keep_source.is_some()
        || fields.source_root.is_some();
    let asset_binary = fields.binary.is_some() && fields.preset.is_none();
    let asset_flags = fields.pattern.is_some() || asset_binary;
    let mut source_recipe = match &github.install {
        Install::Source(source) => Some(source.clone()),
        Install::Asset(_) => options.source_build.clone(),
    };
    let mut asset_recipe = match &github.install {
        Install::Asset(asset) => Some(asset.clone()),
        Install::Source(_) => options.release_asset.clone(),
    };
    if source_flags {
        let base = source_recipe.unwrap_or(SourceInstall {
            dest: previous_dest.clone(),
            ..SourceInstall::default()
        });
        source_recipe = Some(apply_source_fields(base, fields, &github.repo, name)?);
    }
    // Without a build command these settings would be dropped instead of saved.
    let non_empty = |value: Option<&String>| value.is_some_and(|value| !value.trim().is_empty());
    if source_recipe
        .as_ref()
        .is_none_or(|source| source.build.trim().is_empty())
        && let Some((flag, _)) = [
            ("--workdir", non_empty(fields.workdir.as_ref())),
            ("--artifact", non_empty(fields.artifact.as_ref())),
            ("--keep-source", fields.keep_source.is_some()),
            ("--source-root", fields.source_root.is_some()),
        ]
        .into_iter()
        .find(|(_, given)| *given)
    {
        return Err(misuse(
            flag,
            "a GitHub source build; pass --build or --preset",
        ));
    }
    if asset_flags {
        let base = asset_recipe.unwrap_or(AssetInstall {
            dest: previous_dest.clone(),
            ..AssetInstall::default()
        });
        asset_recipe = Some(apply_asset_fields(base, fields, asset_binary));
    }
    let switching = match (&github.install, method) {
        (Install::Source(_), MethodArg::Asset) | (Install::Asset(_), MethodArg::Source) => true,
        (Install::Source(_), MethodArg::Source) | (Install::Asset(_), MethodArg::Asset) => false,
    };
    let install = match method {
        MethodArg::Source => {
            let mut source = source_recipe.unwrap_or(SourceInstall {
                dest: previous_dest.clone(),
                ..SourceInstall::default()
            });
            if switching {
                source.dest.clone_from(&previous_dest);
            }
            source.dest = optional(fields.dest.as_ref(), source.dest);
            source.verify_command = optional(fields.verify_command.as_ref(), source.verify_command);
            options.source_build = None;
            options.release_asset = asset_recipe;
            Install::Source(source)
        }
        MethodArg::Asset => {
            let mut asset = asset_recipe.unwrap_or(AssetInstall {
                dest: previous_dest.clone(),
                ..AssetInstall::default()
            });
            if switching {
                asset.dest.clone_from(&previous_dest);
            }
            asset.dest = optional(fields.dest.as_ref(), asset.dest);
            asset.verify_command = optional(fields.verify_command.as_ref(), asset.verify_command);
            options.source_build = source_recipe.filter(|source| !source.build.trim().is_empty());
            options.release_asset = None;
            Install::Asset(asset)
        }
    };
    Ok((track, install, options))
}

fn apply_source_fields(
    base: SourceInstall,
    fields: &AppFields,
    repo: &str,
    name: &str,
) -> Result<SourceInstall> {
    let base = match &fields.preset {
        Some(id) => preset_source(id, fields.binary.as_deref(), repo, name, base)?,
        None => base,
    };
    Ok(SourceInstall {
        storage: headway_core::SourceStorage {
            keep: fields.keep_source.unwrap_or(base.storage.keep),
            root: optional(fields.source_root.as_ref(), base.storage.root),
        },
        build: fields.build.clone().unwrap_or(base.build),
        workdir: optional(fields.workdir.as_ref(), base.workdir),
        artifact: optional(fields.artifact.as_ref(), base.artifact),
        ..base
    })
}

fn apply_asset_fields(base: AssetInstall, fields: &AppFields, use_binary: bool) -> AssetInstall {
    AssetInstall {
        pattern: optional(fields.pattern.as_ref(), base.pattern),
        binary: if use_binary {
            optional(fields.binary.as_ref(), base.binary)
        } else {
            base.binary
        },
        ..base
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
