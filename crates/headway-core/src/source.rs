//! Where an app's versions come from and how it gets updated.
//!
//! GitHub sources are built or downloaded by Headway itself. npm, Homebrew and
//! script sources hand the update to the tool's own channel and read the installed
//! version back from the system, so updates made outside Headway are seen too.

use serde::{Deserialize, Serialize};

use crate::config::{Install, Track};
use crate::error::{Error, Result};
use crate::repo::RepoId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Source {
    /// Commits, releases or tags of a GitHub repository.
    Github(GithubSource),
    /// A globally installed npm package.
    Npm(NpmSource),
    /// A Homebrew formula or cask.
    Brew(BrewSource),
    /// The project's own update command (`claude update`, `rustup update`, `curl … | sh`).
    Script(ScriptSource),
}

impl Source {
    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Github(a), Self::Github(b)) => {
                a.track.mode() == b.track.mode()
                    && match (a.repo_id(), b.repo_id()) {
                        (Ok(a), Ok(b)) => {
                            a.owner.eq_ignore_ascii_case(&b.owner)
                                && a.name.eq_ignore_ascii_case(&b.name)
                        }
                        _ => false,
                    }
            }
            (Self::Npm(a), Self::Npm(b)) => a.package == b.package,
            (Self::Brew(a), Self::Brew(b)) => a.package == b.package && a.cask == b.cask,
            (Self::Script(a), Self::Script(b)) => a == b,
            _ => false,
        }
    }

    pub const fn method(&self) -> &'static str {
        match self {
            Self::Github(github) => github.install.method(),
            Self::Npm(_) => "npm",
            Self::Brew(_) => "brew",
            Self::Script(_) => "script",
        }
    }
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Github(_) => "github",
            Self::Npm(_) => "npm",
            Self::Brew(_) => "brew",
            Self::Script(_) => "script",
        }
    }

    /// The installed version is read from the system instead of recorded by Headway.
    pub const fn detects_installed(&self) -> bool {
        !matches!(self, Self::Github(_))
    }

    /// Specific versions can be installed (`--next`, `--to`).
    pub const fn picks_versions(&self) -> bool {
        matches!(self, Self::Github(_) | Self::Npm(_))
    }

    /// Short description: `owner/repo`, `npm @scope/pkg`, `brew ripgrep`, `script`.
    pub fn label(&self) -> String {
        match self {
            Self::Github(github) => github.repo.clone(),
            Self::Npm(npm) => format!("npm {}", npm.package),
            Self::Brew(brew) if brew.cask => format!("brew cask {}", brew.package),
            Self::Brew(brew) => format!("brew {}", brew.package),
            Self::Script(_) => "script".to_owned(),
        }
    }

    /// The project's page, when there is a canonical one.
    pub fn web_url(&self) -> Option<String> {
        match self {
            Self::Github(github) => github.repo_id().ok().map(|repo| repo.html_url()),
            Self::Npm(npm) => Some(format!("https://www.npmjs.com/package/{}", npm.package)),
            // Only core formulae and casks have pages on formulae.brew.sh.
            Self::Brew(brew) if !brew.package.contains('/') => Some(format!(
                "https://formulae.brew.sh/{}/{}",
                if brew.cask { "cask" } else { "formula" },
                brew.package
            )),
            Self::Brew(_) | Self::Script(_) => None,
        }
    }

    pub(crate) fn validate(&self) -> std::result::Result<(), String> {
        match self {
            Self::Github(github) => github.validate(),
            Self::Npm(npm) => {
                if !is_npm_package(&npm.package) {
                    return Err(format!("`{}` is not a valid npm package name", npm.package));
                }
                if npm.dist_tag.is_empty()
                    || !npm
                        .dist_tag
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                {
                    return Err(format!("`{}` is not a valid dist-tag", npm.dist_tag));
                }
                Ok(())
            }
            Self::Brew(brew) if is_brew_name(&brew.package) => Ok(()),
            Self::Brew(brew) => Err(format!(
                "`{}` is not a Homebrew name (use `name` or `user/tap/name`)",
                brew.package
            )),
            Self::Script(script) if script.update_command.trim().is_empty() => {
                Err("update_command is empty".to_owned())
            }
            Self::Script(script) => {
                for (name, command) in [
                    ("version_command", &script.version_command),
                    ("latest_command", &script.latest_command),
                ] {
                    if command
                        .as_ref()
                        .is_some_and(|command| command.trim().is_empty())
                    {
                        return Err(format!("{name} must be omitted or contain a command"));
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubSource {
    /// `owner/name` on GitHub.
    pub repo: String,
    /// Clone URL override (e.g. SSH); defaults to `https://github.com/<repo>.git`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    pub track: Track,
    pub install: Install,
    #[serde(flatten)]
    pub options: Box<crate::source_options::GithubOptions>,
}

impl GithubSource {
    pub fn repo_id(&self) -> Result<RepoId> {
        RepoId::parse(&self.repo)
    }

    pub fn clone_url(&self) -> Result<String> {
        match &self.git_url {
            Some(url) => Ok(url.clone()),
            None => Ok(self.repo_id()?.https_clone_url()),
        }
    }

    fn validate(&self) -> std::result::Result<(), String> {
        self.repo_id().map_err(|err| err.to_string())?;
        self.track.tag_regex().map_err(|err| err.to_string())?;
        self.options.validate().map_err(|err| err.to_string())?;
        crate::config::validate_github(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpmSource {
    /// Package name, e.g. `@anthropic-ai/claude-code`.
    pub package: String,
    /// The dist-tag to follow: `latest`, `next`, `beta` …
    #[serde(default = "latest_tag")]
    pub dist_tag: String,
}

fn latest_tag() -> String {
    "latest".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrewSource {
    /// Formula or cask name; `user/tap/name` for third-party taps.
    pub package: String,
    #[serde(default)]
    pub cask: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptSource {
    /// Runs the update, e.g. `claude update` or `curl -fsSL https://… | sh`.
    pub update_command: String,
    /// Prints the installed version, e.g. `claude --version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_command: Option<String>,
    /// Prints the newest version: a shell command, or an http(s) URL whose body has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_command: Option<String>,
}

/// npm's naming rules: lowercase, URL-safe, optionally `@scope/`, at most 214 characters.
fn is_npm_package(name: &str) -> bool {
    let part = |part: &str| {
        !part.is_empty()
            && !part.starts_with(['.', '_', '-'])
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-._~".contains(c))
    };
    let parts_ok = match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, package)| part(scope) && part(package)),
        None => part(name),
    };
    name.len() <= 214 && parts_ok
}

/// `name` or `user/tap/name`.
fn is_brew_name(name: &str) -> bool {
    let segments: Vec<&str> = name.split('/').collect();
    matches!(segments.len(), 1 | 3)
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('-')
                && !matches!(*segment, "." | "..")
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "@+._-".contains(c))
        })
}

/// What `headway add` and the app's add dialog accept: a link or a
/// short form for every source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSpec {
    Github(RepoId),
    Npm(String),
    Brew {
        package: String,
        cask: bool,
    },
    /// A script source; the value names the app.
    Script(String),
}

impl SourceSpec {
    /// `owner/repo` or a github.com URL, `npm:<package>` or an npmjs.com URL,
    /// `brew:<name>` / `cask:<name>` or a formulae.brew.sh URL, `script:<name>`.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim().trim_end_matches('/');
        let invalid = |message: String| Error::InvalidInput(message);
        if let Some(package) = input.strip_prefix("npm:") {
            return Self::npm(package);
        }
        if input.contains("://") {
            let url = reqwest::Url::parse(input)
                .map_err(|err| invalid(format!("invalid source URL: {err}")))?;
            if url.host_str() == Some("github.com")
                && url.scheme() == "ssh"
                && url.username() == "git"
                && url.password().is_none()
            {
                return RepoId::parse(input).map(Self::Github);
            }
            if !matches!(url.scheme(), "https" | "http")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(invalid(
                    "source URLs must use http(s) without credentials".into(),
                ));
            }
            match url.host_str() {
                Some("npmjs.com" | "www.npmjs.com") => {
                    let rest = url
                        .path()
                        .strip_prefix("/package/")
                        .ok_or_else(|| invalid("use an npm package page URL".into()))?;
                    return Self::npm(
                        rest.split("/v/")
                            .next()
                            .unwrap_or(rest)
                            .trim_end_matches('/'),
                    );
                }
                Some("formulae.brew.sh") => {
                    for (prefix, cask) in [("/formula/", false), ("/cask/", true)] {
                        if let Some(package) = url.path().strip_prefix(prefix) {
                            return Self::brew(package.trim_end_matches('/'), cask);
                        }
                    }
                    return Err(invalid("use a Homebrew formula or cask page URL".into()));
                }
                Some("github.com" | "www.github.com") => {
                    return RepoId::parse(url.path().trim_start_matches('/')).map(Self::Github);
                }
                _ => {
                    return Err(invalid(
                        "use a github.com, npmjs.com or formulae.brew.sh source URL".into(),
                    ));
                }
            }
        }
        for (prefix, cask) in [("brew:", false), ("cask:", true)] {
            if let Some(package) = input.strip_prefix(prefix) {
                return Self::brew(package, cask);
            }
        }
        if let Some(name) = input.strip_prefix("script:") {
            let name = name.trim();
            if name.is_empty() {
                return Err(invalid(
                    "`script:` needs a name, e.g. `script:claude`".into(),
                ));
            }
            return Ok(Self::Script(name.to_owned()));
        }
        RepoId::parse(input).map(Self::Github).map_err(|_| {
            invalid(format!(
                "`{input}` is not a source: use owner/repo or a GitHub URL, npm:<package>, \
                 brew:<formula>, cask:<cask>, script:<name>, or an npmjs.com / formulae.brew.sh link"
            ))
        })
    }

    fn npm(package: &str) -> Result<Self> {
        let package = package.trim();
        if is_npm_package(package) {
            Ok(Self::Npm(package.to_owned()))
        } else {
            Err(Error::InvalidInput(format!(
                "`{package}` is not a valid npm package name"
            )))
        }
    }

    fn brew(package: &str, cask: bool) -> Result<Self> {
        let package = package.trim();
        if is_brew_name(package) {
            Ok(Self::Brew {
                package: package.to_owned(),
                cask,
            })
        } else {
            Err(Error::InvalidInput(format!(
                "`{package}` is not a Homebrew name"
            )))
        }
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
