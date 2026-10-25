//! [`Headway`]: the one API the CLI and the desktop app both call, so the two
//! can always do exactly the same things.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use futures_util::{StreamExt as _, stream};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::check;
use crate::check::{Checked, InstalledSync, Probe};
use crate::config::{Config, Settings, Track, TrackedApp};
use crate::doctor::{self, DoctorReport};
use crate::error::{Error, ErrorInfo, Result};
use crate::events::{EventSink, Stage};
use crate::github::{DEFAULT_API, GitHub, RateLimit};
use crate::lock::{self, AppLock};
use crate::paths::{Paths, display_path};
use crate::shell::ShellEnv;
use crate::source::{GithubSource, Source};
use crate::state::{
    AppState, Candidate, CheckResult, HistoryAction, HistoryEntry, InstalledRef, LastError, State,
    UpdateStatus, ids_match,
};
use crate::store::Store;
use crate::token::{KeychainStore, SecretStore, TokenSource};
use crate::update::{self, Job, RunLog, Target, UpdateOutcome, UpdatePlan, short};
use crate::version::Version;

/// Concurrent GitHub checks in `check_many`.
const CHECK_CONCURRENCY: usize = 4;

/// Which environment git and build commands get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvMode {
    /// This process's environment (the CLI, started from a shell).
    Inherit,
    /// An interactive login shell's environment (the app, started from Finder).
    LoginShell,
}

pub struct Headway {
    store: Store,
    secrets: Arc<dyn SecretStore>,
    api: String,
    env_mode: EnvMode,
    env_cache: tokio::sync::Mutex<(u64, Option<ShellEnv>)>,
    pub(crate) commit_metadata_cache: crate::commit_metadata::Cache,
}

pub struct HeadwayBuilder {
    paths: Option<Paths>,
    secrets: Option<Arc<dyn SecretStore>>,
    api: String,
    env_mode: EnvMode,
    env: Option<ShellEnv>,
}

impl HeadwayBuilder {
    #[must_use]
    pub fn paths(self, paths: Paths) -> Self {
        Self {
            paths: Some(paths),
            ..self
        }
    }

    #[must_use]
    pub fn secrets(self, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            secrets: Some(secrets),
            ..self
        }
    }

    #[must_use]
    pub fn api(self, api: impl Into<String>) -> Self {
        Self {
            api: api.into(),
            ..self
        }
    }

    #[must_use]
    pub fn env_mode(self, env_mode: EnvMode) -> Self {
        Self { env_mode, ..self }
    }

    /// Fixes the environment instead of reading it (tests).
    #[must_use]
    pub fn env(self, env: ShellEnv) -> Self {
        Self {
            env: Some(env),
            ..self
        }
    }

    pub fn build(self) -> Result<Headway> {
        let paths = match self.paths {
            Some(paths) => paths,
            None => Paths::from_env()?,
        };
        Ok(Headway {
            store: Store::new(paths),
            secrets: self.secrets.unwrap_or_else(|| Arc::new(KeychainStore)),
            api: self.api,
            env_mode: self.env_mode,
            env_cache: tokio::sync::Mutex::new((0, self.env)),
            commit_metadata_cache: crate::commit_metadata::Cache::default(),
        })
    }
}

mod apps;
mod check_summary;
mod checks;
mod commits;
#[cfg(test)]
mod commits_release_tests;
#[cfg(test)]
mod engine_recovery_tests;
#[cfg(test)]
mod engine_regressions;
mod github;
mod installation;
mod inventory;
mod matching;
#[cfg(test)]
mod matching_tests;
#[path = "update.rs"]
mod operations;
mod policy;
mod presence;
#[cfg(test)]
mod presence_tests;
#[cfg(test)]
mod pull_request_tests;
mod pull_requests;
mod recovery;
mod repositories;
#[cfg(test)]
mod repository_switch_tests;
mod resolve;
mod revision_install;
mod revision_prepare;
#[cfg(test)]
mod source_tests;
#[cfg(test)]
mod tests;
mod update_method;
#[cfg(test)]
mod update_method_tests;
mod variant_launch;
mod variant_prepare;
mod variants;
mod views;

pub use check_summary::{CheckFailure, CheckLatest, CheckRecord, CheckSummary};
pub use installation::InstallationDetection;
pub use policy::AutoUpdateReport;
pub use presence::Presence;
pub use pull_requests::PullRequestPage;
pub use views::{AppStatus, AppView, LogFile, RemovedApp, TokenStatus};

impl Headway {
    /// Identity of this binary; available without opening a store.
    #[must_use]
    pub fn build_info() -> crate::BuildInfo {
        crate::BuildInfo::current()
    }

    pub fn builder() -> HeadwayBuilder {
        HeadwayBuilder {
            paths: None,
            secrets: None,
            api: DEFAULT_API.to_owned(),
            env_mode: EnvMode::Inherit,
            env: None,
        }
    }

    pub const fn paths(&self) -> &Paths {
        self.store.paths()
    }

    pub const fn store(&self) -> &Store {
        &self.store
    }

    /// The environment for git and builds (cached; see [`EnvMode`]).
    pub async fn env(&self) -> ShellEnv {
        let generation = {
            let cache = self.env_cache.lock().await;
            if let Some(env) = cache.1.as_ref() {
                return env.clone();
            }
            cache.0
        };
        // Shell startup may take seconds: never hold the cache guard across it.
        let env = match self.env_mode {
            EnvMode::Inherit => ShellEnv::current(),
            EnvMode::LoginShell => ShellEnv::login_shell().await,
        };
        let mut cache = self.env_cache.lock().await;
        if cache.0 == generation {
            cache.1.get_or_insert_with(|| env.clone()).clone()
        } else {
            env
        }
    }

    /// Forgets the cached environment (after the user changed their shell setup).
    pub async fn reload_env(&self) {
        let mut cache = self.env_cache.lock().await;
        cache.0 = cache.0.wrapping_add(1);
        cache.1 = None;
    }

    // ---- diagnostics -------------------------------------------------------

    pub async fn doctor(&self) -> DoctorReport {
        let env = self.env().await;
        let config_check = match self.store.config() {
            Ok(config) => doctor::DoctorCheck {
                name: "config".into(),
                status: doctor::CheckStatus::Ok,
                message: format!(
                    "{} app(s) tracked in {}",
                    config.apps.len(),
                    display_path(&self.paths().config_file())
                ),
                hint: None,
            },
            Err(error) => doctor::error_check("config", &error),
        };
        let state_check = match self.store.state() {
            Ok(_) => doctor::DoctorCheck {
                name: "state".into(),
                status: doctor::CheckStatus::Ok,
                message: format!(
                    "State schema is supported; recovery backup: {}",
                    self.paths()
                        .state_file()
                        .with_extension("json.bak")
                        .display()
                ),
                hint: None,
            },
            Err(error) => doctor::error_check("state", &error),
        };
        let mut checks = vec![config_check, state_check];
        let uses_github = self.store.config().is_ok_and(|config| {
            config.apps.is_empty() || config.apps.iter().any(|app| app.github().is_some())
        });
        if uses_github {
            checks.push(doctor::git_check(&env).await);
            checks.push(match self.token_status().await {
                Ok(status) => doctor::token_check(&status),
                Err(err) => doctor::error_check("github", &err),
            });
        }
        if let Ok(config) = self.store.config() {
            checks.push(doctor::bin_dir_check(&config.settings, &env));
            checks.push(doctor::apps_dir_check(&config.settings));
            checks.extend(
                config
                    .apps
                    .iter()
                    .filter_map(|app| doctor::tools_check(app, &env)),
            );
        }
        DoctorReport::new(checks)
    }
}

/// Validates `app` and stores the repository as `owner/name`, whatever URL form it came in.
fn normalized(app: TrackedApp) -> Result<TrackedApp> {
    app.validate()?;
    let Some(github) = app.github() else {
        return Ok(app);
    };
    let dest = match &github.install {
        crate::config::Install::Source(source) => source.dest.as_deref(),
        crate::config::Install::Asset(asset) => asset.dest.as_deref(),
    };
    if let Some(dest) = dest {
        let path = crate::paths::expand_tilde(dest);
        if path.is_dir() && !path.is_symlink() && !crate::install::is_bundle(&path) {
            return Err(Error::InvalidInput(format!(
                "dest `{dest}` is an existing directory; give the full path of the installed \
                 file, e.g. {dest}/<name>"
            )));
        }
    }
    let repo = app.repo_id()?.full_name();
    Ok(TrackedApp {
        source: Source::Github(GithubSource {
            repo,
            ..github.clone()
        }),
        ..app
    })
}

/// One update run's inputs.
#[derive(Clone, Copy)]
struct Attempt<'a> {
    app: &'a TrackedApp,
    settings: &'a Settings,
    installed: Option<&'a InstalledRef>,
    target: &'a Target,
    /// The recipe used for this run without changing the saved tracking policy.
    recipe: &'a TrackedApp,
    log: &'a RunLog<'a>,
    cancel: &'a CancellationToken,
}

fn candidate_error(error: Error) -> Error {
    match error {
        Error::Config(message) => Error::InvalidInput(message),
        other => other,
    }
}

fn target_matches(app: &TrackedApp, installed: &str, target: &str) -> bool {
    match app.github().map(|github| &github.track) {
        Some(Track::Commit { .. }) => ids_match(installed, target),
        _ => installed == target,
    }
}

fn ref_label(installed: &InstalledRef) -> String {
    match &installed.version {
        Some(version) if version != &installed.id => {
            format!("{version} ({})", short(&installed.id))
        }
        _ => short(&installed.id),
    }
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(join_error)`
fn join_error(err: tokio::task::JoinError) -> Error {
    Error::io("background task", std::io::Error::other(err))
}
