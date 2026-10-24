//! User-editable configuration (`config.toml`): global settings and tracked apps.
//!
//! Both the CLI and the desktop app read and write this file, so everything here
//! is plain data with validation; behaviour lives in the engine modules.

use std::path::{Component, Path};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::repo::{RepoId, validate_app_id};
use crate::source::{GithubSource, Source};

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

/// One app or tool Headway keeps up to date.
///
/// The source's fields sit next to these (`source = "npm"`, `package = "…"`), so a
/// GitHub app reads `source = "github"`, `repo`, `track`, `install`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackedApp {
    /// Stable id used by the CLI and in file names.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: AppKind,
    /// Install updates as soon as a check finds them (desktop app only).
    #[serde(default)]
    pub auto_update: bool,
    #[serde(flatten)]
    pub source: Source,
}

impl<'de> Deserialize<'de> for TrackedApp {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        // Older configs predate `source`; their GitHub fields keep the same layout.
        #[derive(Deserialize)]
        struct Wire {
            id: String,
            name: String,
            #[serde(default)]
            kind: AppKind,
            #[serde(default)]
            auto_update: bool,
            #[serde(flatten)]
            source: Source,
        }
        let mut value = serde_json::Value::deserialize(deserializer)?;
        if let Some(map) = value.as_object_mut() {
            map.entry("source").or_insert_with(|| "github".into());
        }
        let wire: Wire = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: wire.id,
            name: wire.name,
            kind: wire.kind,
            auto_update: wire.auto_update,
            source: wire.source,
        })
    }
}

impl TrackedApp {
    pub const fn github(&self) -> Option<&GithubSource> {
        match &self.source {
            Source::Github(github) => Some(github),
            Source::Npm(_) | Source::Brew(_) | Source::Script(_) => None,
        }
    }

    /// The GitHub source, or an error saying `what` needs one.
    pub fn require_github(&self, what: &str) -> Result<&GithubSource> {
        self.github().ok_or_else(|| {
            Error::InvalidInput(format!(
                "{what} only applies to GitHub sources; `{}` follows {}",
                self.id,
                self.source.label()
            ))
        })
    }

    pub fn repo_id(&self) -> Result<RepoId> {
        self.require_github("a repository")?.repo_id()
    }

    pub fn validate(&self) -> Result<()> {
        let fail = |message: String| Error::Config(format!("app `{}`: {message}", self.id));
        validate_app_id(&self.id).map_err(|err| fail(err.to_string()))?;
        if self.name.trim().is_empty() {
            return Err(fail("name is empty".into()));
        }
        self.source.validate().map_err(fail)
    }
}

/// Checks the GitHub-specific fields (branch, git URL, build/asset settings).
pub(crate) fn validate_github(github: &GithubSource) -> std::result::Result<(), String> {
    if let Track::Commit {
        branch: Some(branch),
    } = &github.track
        && (branch.trim().is_empty() || branch.contains(char::is_whitespace))
    {
        return Err(format!("branch `{branch}` is not a valid branch name"));
    }
    let accepted = ["https://", "http://", "ssh://", "git@", "file://", "/"];
    if let Some(url) = &github.git_url
        && !accepted.iter().any(|prefix| url.starts_with(prefix))
    {
        return Err(format!("git_url `{url}` is not a git URL"));
    }
    match &github.install {
        Install::Source(source) => {
            if source.build.trim().is_empty() {
                return Err("install.build is empty".into());
            }
            for (field, value) in [("workdir", &source.workdir), ("artifact", &source.artifact)] {
                if let Some(value) = value
                    && !is_inside_repo(value)
                {
                    return Err(format!(
                        "install.{field} `{value}` must be a relative path inside the repository"
                    ));
                }
            }
            validate_dest(source.dest.as_deref()).map_err(|err| err.to_string())
        }
        Install::Asset(asset) => {
            if !matches!(github.track, Track::Release { .. }) {
                return Err(format!(
                    "install.method = \"asset\" needs track.mode = \"release\" (got \"{}\")",
                    github.track.mode()
                ));
            }
            if let Some(binary) = &asset.binary
                && (binary.is_empty() || binary.contains('/'))
            {
                return Err(format!(
                    "install.binary `{binary}` must be a plain file name"
                ));
            }
            validate_dest(asset.dest.as_deref()).map_err(|err| err.to_string())
        }
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
#[path = "config_tests.rs"]
mod tests;
