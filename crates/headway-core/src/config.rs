//! User-editable configuration (`config.toml`): global settings and tracked apps.
//!
//! Both the CLI and the desktop app read and write this file, so everything here
//! is plain data with validation; behaviour lives in the engine modules.

use std::path::{Component, Path};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::repo::{RepoId, validate_app_id};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default, rename = "app")]
    pub apps: Vec<TrackedApp>,
}

impl Config {
    pub fn app(&self, id: &str) -> Result<&TrackedApp> {
        self.apps
            .iter()
            .find(|app| app.id == id)
            .ok_or_else(|| Error::AppNotFound(id.to_owned()))
    }

    /// Validates every app and rejects duplicate ids.
    pub fn validate(&self) -> Result<()> {
        for (index, app) in self.apps.iter().enumerate() {
            app.validate()?;
            if self.apps[..index].iter().any(|other| other.id == app.id) {
                return Err(Error::Config(format!("app id `{}` is used twice", app.id)));
            }
        }
        Ok(())
    }
}

/// Global settings shared by the CLI and the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Minutes between background checks in the desktop app; `0` turns them off.
    pub check_interval_minutes: u32,
    /// Notify when new commits or releases show up.
    pub notify: bool,
    /// Where CLI binaries are installed unless an app sets `dest`.
    pub bin_dir: String,
    /// Where `.app` bundles are installed unless an app sets `dest`.
    pub apps_dir: String,
    /// Use `gh auth token` when no token is configured.
    pub gh_fallback: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            check_interval_minutes: 60,
            notify: true,
            bin_dir: "~/.local/bin".to_owned(),
            apps_dir: "/Applications".to_owned(),
            gh_fallback: true,
        }
    }
}

impl Settings {
    pub const KEYS: [&'static str; 5] = [
        "check_interval_minutes",
        "notify",
        "bin_dir",
        "apps_dir",
        "gh_fallback",
    ];

    /// Returns a copy with `key` set from its string form (as typed on the command line).
    pub fn with_key(&self, key: &str, value: &str) -> Result<Self> {
        let value = value.trim();
        let next = match key {
            "check_interval_minutes" => Self {
                check_interval_minutes: value.parse().map_err(|_| {
                    Error::InvalidInput(format!("`{value}` is not a number of minutes"))
                })?,
                ..self.clone()
            },
            "notify" => Self {
                notify: parse_bool(value)?,
                ..self.clone()
            },
            "gh_fallback" => Self {
                gh_fallback: parse_bool(value)?,
                ..self.clone()
            },
            "bin_dir" => Self {
                bin_dir: non_empty_path(value)?,
                ..self.clone()
            },
            "apps_dir" => Self {
                apps_dir: non_empty_path(value)?,
                ..self.clone()
            },
            _ => {
                return Err(Error::InvalidInput(format!(
                    "unknown setting `{key}`; expected one of {}",
                    Self::KEYS.join(", ")
                )));
            }
        };
        Ok(next)
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(Error::InvalidInput(format!("`{value}` is not true/false"))),
    }
}

fn non_empty_path(value: &str) -> Result<String> {
    if value.starts_with('/') || value.starts_with('~') {
        Ok(value.to_owned())
    } else {
        Err(Error::InvalidInput(format!(
            "`{value}` must be an absolute path or start with ~"
        )))
    }
}

/// What the tracked repository produces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppKind {
    /// A command-line binary, installed into `bin_dir`.
    #[default]
    Cli,
    /// A macOS `.app` bundle, installed into `apps_dir`.
    App,
}

/// Which upstream changes count as updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Track {
    /// Every commit on a branch (`None` means the repository's default branch).
    Commit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    /// GitHub releases, newest version first.
    Release {
        #[serde(default)]
        prerelease: bool,
        /// Regex a tag must match, e.g. `^cli-v` in a monorepo.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag_filter: Option<String>,
    },
    /// Git tags that look like versions (for repositories without releases).
    Tag {
        #[serde(default)]
        prerelease: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag_filter: Option<String>,
    },
}

impl Track {
    pub const fn mode(&self) -> &'static str {
        match self {
            Self::Commit { .. } => "commit",
            Self::Release { .. } => "release",
            Self::Tag { .. } => "tag",
        }
    }

    pub fn tag_filter(&self) -> Option<&str> {
        match self {
            Self::Commit { .. } => None,
            Self::Release { tag_filter, .. } | Self::Tag { tag_filter, .. } => {
                tag_filter.as_deref()
            }
        }
    }

    /// Compiled `tag_filter`, if one is set.
    pub fn tag_regex(&self) -> Result<Option<Regex>> {
        self.tag_filter()
            .map(|pattern| {
                Regex::new(pattern)
                    .map_err(|err| Error::Config(format!("tag_filter `{pattern}`: {err}")))
            })
            .transpose()
    }
}

/// How a new version gets onto the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Install {
    /// Clone, check out the target ref, run `build`, then copy `artifact` to `dest`.
    Source(SourceInstall),
    /// Download a release asset and install the binary or `.app` inside it.
    Asset(AssetInstall),
}

impl Install {
    pub const fn method(&self) -> &'static str {
        match self {
            Self::Source(_) => "source",
            Self::Asset(_) => "asset",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInstall {
    /// Shell command run with `/bin/sh -c` in `workdir`.
    pub build: String,
    /// Directory inside the repository to build in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    /// Build output to install, relative to `workdir`; `*` matches within the last segment.
    /// Leave unset when `build` installs the program itself (e.g. `cargo install`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
    /// Install destination; defaults to `bin_dir`/`apps_dir` plus the artifact's file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInstall {
    /// Glob for the asset file name; unset picks the best macOS asset for this CPU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Binary (CLI) or bundle (`Foo.app`) to take from the asset; defaults to the repo name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    /// Install destination; defaults to `bin_dir`/`apps_dir` plus the payload's file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
}

/// One repository Headway keeps installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedApp {
    /// Stable id used by the CLI and in file names.
    pub id: String,
    pub name: String,
    /// `owner/name` on GitHub.
    pub repo: String,
    #[serde(default)]
    pub kind: AppKind,
    /// Install updates as soon as a check finds them (desktop app only).
    #[serde(default)]
    pub auto_update: bool,
    /// Clone URL override (e.g. SSH); defaults to `https://github.com/<repo>.git`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    pub track: Track,
    pub install: Install,
}

impl TrackedApp {
    pub fn repo_id(&self) -> Result<RepoId> {
        RepoId::parse(&self.repo)
    }

    pub fn clone_url(&self) -> Result<String> {
        match &self.git_url {
            Some(url) => Ok(url.clone()),
            None => Ok(self.repo_id()?.https_clone_url()),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Error::Config(format!("app `{}`: {message}", self.id));

        validate_app_id(&self.id).map_err(|err| fail(err.to_string()))?;
        if self.name.trim().is_empty() {
            return Err(fail("name is empty".into()));
        }
        self.repo_id().map_err(|err| fail(err.to_string()))?;
        self.track.tag_regex()?;
        if let Track::Commit {
            branch: Some(branch),
        } = &self.track
            && (branch.trim().is_empty() || branch.contains(char::is_whitespace))
        {
            return Err(fail(format!(
                "branch `{branch}` is not a valid branch name"
            )));
        }
        let accepted = ["https://", "http://", "ssh://", "git@", "file://", "/"];
        if let Some(url) = &self.git_url
            && !accepted.iter().any(|prefix| url.starts_with(prefix))
        {
            return Err(fail(format!("git_url `{url}` is not a git URL")));
        }

        match &self.install {
            Install::Source(source) => {
                if source.build.trim().is_empty() {
                    return Err(fail("install.build is empty".into()));
                }
                for (field, value) in [("workdir", &source.workdir), ("artifact", &source.artifact)]
                {
                    if let Some(value) = value
                        && !is_inside_repo(value)
                    {
                        return Err(fail(format!(
                            "install.{field} `{value}` must be a relative path inside the repository"
                        )));
                    }
                }
                validate_dest(source.dest.as_deref()).map_err(|err| fail(err.to_string()))?;
            }
            Install::Asset(asset) => {
                if !matches!(self.track, Track::Release { .. }) {
                    return Err(fail(format!(
                        "install.method = \"asset\" needs track.mode = \"release\" (got \"{}\")",
                        self.track.mode()
                    )));
                }
                if let Some(binary) = &asset.binary
                    && (binary.is_empty() || binary.contains('/'))
                {
                    return Err(fail(format!(
                        "install.binary `{binary}` must be a plain file name"
                    )));
                }
                validate_dest(asset.dest.as_deref()).map_err(|err| fail(err.to_string()))?;
            }
        }
        Ok(())
    }
}

fn is_inside_repo(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn validate_dest(dest: Option<&str>) -> Result<()> {
    match dest {
        Some(dest) if !(dest.starts_with('/') || dest.starts_with("~/")) => Err(Error::Config(
            format!("install.dest `{dest}` must be an absolute path or start with ~/"),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::cli_app;

    #[test]
    fn toml_round_trip_keeps_every_field() {
        let config = Config {
            settings: Settings {
                check_interval_minutes: 15,
                ..Settings::default()
            },
            apps: vec![
                cli_app("rg"),
                TrackedApp {
                    id: "zed".into(),
                    name: "Zed".into(),
                    repo: "zed-industries/zed".into(),
                    kind: AppKind::App,
                    auto_update: true,
                    git_url: Some("git@github.com:zed-industries/zed.git".into()),
                    track: Track::Release {
                        prerelease: true,
                        tag_filter: Some("^v".into()),
                    },
                    install: Install::Asset(AssetInstall {
                        pattern: Some("Zed-aarch64.dmg".into()),
                        binary: None,
                        dest: Some("~/Applications/Zed.app".into()),
                    }),
                },
            ],
        };
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
        parsed.validate().unwrap();
    }

    #[test]
    fn hand_written_toml_uses_defaults() {
        let text = r#"
            [[app]]
            id = "rg"
            name = "ripgrep"
            repo = "BurntSushi/ripgrep"
            track = { mode = "commit" }
            install = { method = "source", build = "cargo build --release", artifact = "target/release/rg" }
        "#;
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.settings, Settings::default());
        assert_eq!(config.apps[0].kind, AppKind::Cli);
        assert!(!config.apps[0].auto_update);
        config.validate().unwrap();
    }

    #[test]
    fn rejects_duplicate_ids() {
        let config = Config {
            settings: Settings::default(),
            apps: vec![cli_app("rg"), cli_app("rg")],
        };
        assert!(matches!(config.validate(), Err(Error::Config(msg)) if msg.contains("twice")));
    }

    #[test]
    fn asset_installs_require_release_tracking() {
        let app = TrackedApp {
            install: Install::Asset(AssetInstall::default()),
            ..cli_app("rg")
        };
        assert!(app.validate().is_err());
        let app = TrackedApp {
            track: Track::Release {
                prerelease: false,
                tag_filter: None,
            },
            ..app
        };
        app.validate().unwrap();
    }

    #[test]
    fn source_paths_must_stay_inside_the_repository() {
        for artifact in ["../evil", "/abs/path", ""] {
            let app = TrackedApp {
                install: Install::Source(SourceInstall {
                    build: "make".into(),
                    artifact: Some(artifact.into()),
                    ..SourceInstall::default()
                }),
                ..cli_app("x")
            };
            assert!(app.validate().is_err(), "{artifact}");
        }
    }

    #[test]
    fn rejects_bad_fields() {
        let bad_regex = TrackedApp {
            track: Track::Tag {
                prerelease: false,
                tag_filter: Some("(".into()),
            },
            ..cli_app("x")
        };
        assert!(bad_regex.validate().is_err());

        let bad_dest = TrackedApp {
            install: Install::Source(SourceInstall {
                build: "make".into(),
                dest: Some("relative/bin".into()),
                ..SourceInstall::default()
            }),
            ..cli_app("x")
        };
        assert!(bad_dest.validate().is_err());

        let empty_build = TrackedApp {
            install: Install::Source(SourceInstall::default()),
            ..cli_app("x")
        };
        assert!(empty_build.validate().is_err());

        let bad_branch = TrackedApp {
            track: Track::Commit {
                branch: Some("two words".into()),
            },
            ..cli_app("x")
        };
        assert!(bad_branch.validate().is_err());
    }

    #[test]
    fn settings_keys_parse_from_strings() {
        let base = Settings::default();
        assert_eq!(
            base.with_key("check_interval_minutes", "30")
                .unwrap()
                .check_interval_minutes,
            30
        );
        assert!(!base.with_key("notify", "off").unwrap().notify);
        assert_eq!(base.with_key("bin_dir", "~/bin").unwrap().bin_dir, "~/bin");
        assert!(base.with_key("bin_dir", "bin").is_err());
        assert!(base.with_key("notify", "maybe").is_err());
        assert!(base.with_key("nope", "1").is_err());
        assert_eq!(base, Settings::default(), "with_key must not mutate");
    }

    #[test]
    fn clone_url_prefers_override() {
        let app = cli_app("rg");
        assert_eq!(app.clone_url().unwrap(), "https://github.com/owner/rg.git");
        let ssh = TrackedApp {
            git_url: Some("git@github.com:owner/rg.git".into()),
            ..app
        };
        assert_eq!(ssh.clone_url().unwrap(), "git@github.com:owner/rg.git");
    }
}
