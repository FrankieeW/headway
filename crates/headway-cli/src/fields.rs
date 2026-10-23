//! Turning `add`/`edit` flags into a config, on top of a base (the inspection's
//! suggestion, or the current config). Flags that don't fit the chosen mode are
//! rejected instead of being silently ignored.

use headway_core::config::{AppKind, AssetInstall, Install, SourceInstall, Track, TrackedApp};
use headway_core::presets;
use headway_core::{Error, Result};

use crate::args::{AppFields, KindArg, MethodArg, TrackArg};

/// `None` keeps the base value, `Some("")` clears it, anything else sets it.
fn optional(flag: Option<&String>, base: Option<String>) -> Option<String> {
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
        None => base.kind,
    };
    let name = fields.name.clone().unwrap_or(base.name);
    let track = apply_track(&base.track, fields)?;
    let install = apply_install(base.install, &track, fields, &base.repo, &name)?;
    Ok(TrackedApp {
        name,
        kind,
        auto_update: fields.auto_update.unwrap_or(base.auto_update),
        git_url: optional(fields.git_url.as_ref(), base.git_url),
        track,
        install,
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
        build: presets::render(preset.build, &bin, name),
        artifact: preset
            .artifact
            .map(|artifact| presets::render(artifact, &bin, name)),
        ..base
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> TrackedApp {
        TrackedApp {
            id: "rg".into(),
            name: "ripgrep".into(),
            repo: "BurntSushi/ripgrep".into(),
            kind: AppKind::Cli,
            auto_update: false,
            git_url: None,
            track: Track::Commit { branch: None },
            install: Install::Source(SourceInstall {
                build: "cargo build --release --locked".into(),
                artifact: Some("target/release/rg".into()),
                ..SourceInstall::default()
            }),
        }
    }

    #[test]
    fn no_flags_keep_the_base() {
        assert_eq!(apply(base(), &AppFields::default()).unwrap(), base());
    }

    #[test]
    fn switching_to_release_assets() {
        let fields = AppFields {
            track: Some(TrackArg::Release),
            method: Some(MethodArg::Asset),
            pattern: Some("*aarch64-apple-darwin.tar.gz".into()),
            binary: Some("rg".into()),
            prerelease: Some(true),
            ..AppFields::default()
        };
        let app = apply(base(), &fields).unwrap();
        assert_eq!(
            app.track,
            Track::Release {
                prerelease: true,
                tag_filter: None
            }
        );
        assert_eq!(
            app.install,
            Install::Asset(AssetInstall {
                pattern: Some("*aarch64-apple-darwin.tar.gz".into()),
                binary: Some("rg".into()),
                dest: None,
            })
        );
        app.validate().unwrap();
    }

    #[test]
    fn leaving_release_mode_switches_assets_back_to_source() {
        let release = apply(
            base(),
            &AppFields {
                track: Some(TrackArg::Release),
                method: Some(MethodArg::Asset),
                ..AppFields::default()
            },
        )
        .unwrap();
        let tags = apply(
            release,
            &AppFields {
                track: Some(TrackArg::Tag),
                build: Some("make".into()),
                ..AppFields::default()
            },
        )
        .unwrap();
        assert!(matches!(&tags.install, Install::Source(source) if source.build == "make"));
    }

    #[test]
    fn presets_fill_the_build_and_flags_override() {
        let fields = AppFields {
            preset: Some("go".into()),
            binary: Some("lazygit".into()),
            dest: Some("~/bin/lg".into()),
            ..AppFields::default()
        };
        let app = apply(base(), &fields).unwrap();
        let Install::Source(source) = app.install else {
            panic!("source")
        };
        assert_eq!(source.build, "go build -trimpath -o .headway-out/lazygit .");
        assert_eq!(source.artifact.as_deref(), Some(".headway-out/lazygit"));
        assert_eq!(source.dest.as_deref(), Some("~/bin/lg"));
    }

    #[test]
    fn empty_strings_clear_optional_fields() {
        let fields = AppFields {
            artifact: Some(String::new()),
            branch: Some("dev".into()),
            ..AppFields::default()
        };
        let app = apply(base(), &fields).unwrap();
        assert_eq!(
            app.track,
            Track::Commit {
                branch: Some("dev".into())
            }
        );
        assert!(matches!(&app.install, Install::Source(source) if source.artifact.is_none()));
        let cleared = apply(
            app,
            &AppFields {
                branch: Some(String::new()),
                ..AppFields::default()
            },
        )
        .unwrap();
        assert_eq!(cleared.track, Track::Commit { branch: None });
    }

    #[test]
    fn mismatched_flags_are_rejected() {
        let cases = [
            AppFields {
                branch: Some("x".into()),
                track: Some(TrackArg::Release),
                ..AppFields::default()
            },
            AppFields {
                prerelease: Some(true),
                ..AppFields::default()
            },
            AppFields {
                pattern: Some("*".into()),
                ..AppFields::default()
            },
            AppFields {
                track: Some(TrackArg::Release),
                method: Some(MethodArg::Asset),
                build: Some("make".into()),
                ..AppFields::default()
            },
            AppFields {
                preset: Some("nope".into()),
                ..AppFields::default()
            },
        ];
        for fields in cases {
            assert!(
                matches!(apply(base(), &fields), Err(Error::InvalidInput(_))),
                "{fields:?}"
            );
        }
    }
}
