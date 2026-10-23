//! [`Headway`]: the one API the CLI and the desktop app both call, so the two
//! can always do exactly the same things.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use futures_util::{StreamExt as _, stream};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::check;
use crate::config::{Config, Settings, Track, TrackedApp};
use crate::doctor::{self, DoctorReport};
use crate::error::{Error, ErrorInfo, Result};
use crate::events::{EventSink, Stage};
use crate::github::{DEFAULT_API, GitHub, RateLimit};
use crate::inspect::{self, RepoInspection};
use crate::lock::{self, AppLock};
use crate::paths::{Paths, display_path};
use crate::shell::ShellEnv;
use crate::state::{
    AppState, Candidate, CheckResult, HistoryAction, HistoryEntry, InstalledRef, LastError, State,
    UpdateStatus, ids_match,
};
use crate::store::Store;
use crate::token::{self, KeychainStore, ResolvedToken, SecretStore, TokenSource};
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
    env_cache: tokio::sync::Mutex<Option<ShellEnv>>,
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
            env_cache: tokio::sync::Mutex::new(self.env),
        })
    }
}

/// One app as the CLI and the UI show it.
#[derive(Debug, Clone, Serialize)]
pub struct AppView {
    pub app: TrackedApp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<InstalledRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<CheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<LastError>,
    /// An update is running right now (in any process).
    pub busy: bool,
    pub status: AppStatus,
    /// Newer commits/releases known from the last check.
    pub pending_count: u32,
}

/// A single status for badges and scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Unchecked,
    UpToDate,
    UpdateAvailable,
    NotInstalled,
    Diverged,
    Error,
    Busy,
}

impl AppView {
    fn new(app: TrackedApp, state: AppState, busy: bool) -> Self {
        let status = if busy {
            AppStatus::Busy
        } else if state.last_error.is_some() {
            AppStatus::Error
        } else {
            match state.last_check.as_ref().map(|check| check.status) {
                None => AppStatus::Unchecked,
                Some(UpdateStatus::UpToDate) => AppStatus::UpToDate,
                Some(UpdateStatus::UpdateAvailable) => AppStatus::UpdateAvailable,
                Some(UpdateStatus::NotInstalled) => AppStatus::NotInstalled,
                Some(UpdateStatus::Diverged) => AppStatus::Diverged,
            }
        };
        let pending_count = state
            .last_check
            .as_ref()
            .filter(|check| {
                matches!(
                    check.status,
                    UpdateStatus::UpdateAvailable | UpdateStatus::Diverged
                )
            })
            .map_or(0, |check| check.pending_total);
        Self {
            app,
            installed: state.installed,
            last_check: state.last_check,
            last_error: state.last_error,
            busy,
            status,
            pending_count,
        }
    }

    pub const fn has_update(&self) -> bool {
        self.pending_count > 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenStatus {
    pub source: TokenSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<RateLimit>,
    /// Why the token could not be verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemovedApp {
    pub id: String,
    pub purged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogFile {
    pub path: String,
    pub content: String,
}

impl Headway {
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
        let mut cache = self.env_cache.lock().await;
        if let Some(env) = cache.as_ref() {
            return env.clone();
        }
        let env = match self.env_mode {
            EnvMode::Inherit => ShellEnv::current(),
            EnvMode::LoginShell => ShellEnv::login_shell().await,
        };
        *cache = Some(env.clone());
        env
    }

    /// Forgets the cached environment (after the user changed their shell setup).
    pub async fn reload_env(&self) {
        *self.env_cache.lock().await = None;
    }

    // ---- apps --------------------------------------------------------------

    pub fn list(&self) -> Result<Vec<AppView>> {
        let config = self.store.config()?;
        let state = self.store.state()?;
        Ok(config
            .apps
            .iter()
            .map(|app| self.view(app, &state))
            .collect())
    }

    pub fn get(&self, id: &str) -> Result<AppView> {
        let config = self.store.config()?;
        Ok(self.view(config.app(id)?, &self.store.state()?))
    }

    fn view(&self, app: &TrackedApp, state: &State) -> AppView {
        let busy = lock::is_locked(&self.paths().app_lock(&app.id));
        AppView::new(app.clone(), state.app(&app.id), busy)
    }

    pub fn add(&self, app: TrackedApp) -> Result<AppView> {
        let app = normalized(app)?;
        let id = app.id.clone();
        self.store.update_config(|Config { settings, apps }| {
            if apps.iter().any(|existing| existing.id == app.id) {
                return Err(Error::AppExists(app.id));
            }
            let apps = apps.into_iter().chain(std::iter::once(app)).collect();
            Ok((Config { settings, apps }, ()))
        })?;
        self.get(&id)
    }

    /// Replaces an app's config. Changing what is tracked forgets the installed ref,
    /// since it no longer describes the same thing.
    pub fn replace(&self, app: TrackedApp) -> Result<AppView> {
        let app = normalized(app)?;
        let id = app.id.clone();
        let previous = self.store.update_config(|Config { settings, apps }| {
            let index = apps
                .iter()
                .position(|existing| existing.id == app.id)
                .ok_or_else(|| Error::AppNotFound(app.id.clone()))?;
            // `apps` is this closure's own copy of the config, swapped in one slot.
            let mut apps = apps;
            let previous = std::mem::replace(&mut apps[index], app);
            Ok((Config { settings, apps }, previous))
        })?;
        let new = self.store.config()?.app(&id)?.clone();
        let identity_changed =
            previous.repo != new.repo || previous.track.mode() != new.track.mode();
        let check_stale = identity_changed || previous.track != new.track;
        if check_stale {
            self.store.update_state(|state| {
                let next = state.with_app(&id, |app_state| AppState {
                    installed: if identity_changed {
                        None
                    } else {
                        app_state.installed
                    },
                    last_check: None,
                    last_error: None,
                    ..app_state
                });
                Ok((next, ()))
            })?;
        }
        self.get(&id)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<RemovedApp> {
        if lock::is_locked(&self.paths().app_lock(id)) {
            return Err(Error::Busy(id.to_owned()));
        }
        self.store.update_config(|Config { settings, apps }| {
            if !apps.iter().any(|app| app.id == id) {
                return Err(Error::AppNotFound(id.to_owned()));
            }
            let apps = apps.into_iter().filter(|app| app.id != id).collect();
            Ok((Config { settings, apps }, ()))
        })?;
        self.store
            .update_state(|state| Ok((state.without_app(id), ())))?;
        if purge {
            for dir in [self.paths().source_dir(id), self.paths().app_log_dir(id)] {
                if dir.exists() {
                    std::fs::remove_dir_all(&dir)
                        .map_err(|err| Error::io_at("remove", &dir, err))?;
                }
            }
        }
        Ok(RemovedApp {
            id: id.to_owned(),
            purged: purge,
        })
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self.store.config()?.settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<Settings> {
        // Round-trip every field through the same validation as `settings set`.
        let validated = Settings::KEYS
            .iter()
            .try_fold(Settings::default(), |acc, key| {
                acc.with_key(key, &setting_value(settings, key))
            })?;
        self.store.update_config(|config| {
            Ok((
                Config {
                    settings: validated.clone(),
                    ..config
                },
                (),
            ))
        })?;
        Ok(validated)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<Settings> {
        self.store.update_config(|config| {
            let settings = config.settings.with_key(key, value)?;
            Ok((
                Config {
                    settings: settings.clone(),
                    ..config
                },
                settings,
            ))
        })
    }

    pub fn history(&self, id: &str) -> Result<Vec<HistoryEntry>> {
        self.store.config()?.app(id)?;
        let mut history = self.store.state()?.app(id).history;
        history.reverse();
        Ok(history)
    }

    pub fn last_log(&self, id: &str) -> Result<Option<LogFile>> {
        self.store.config()?.app(id)?;
        let Some(path) = update::latest_log(&self.paths().app_log_dir(id)) else {
            return Ok(None);
        };
        let content =
            std::fs::read_to_string(&path).map_err(|err| Error::io_at("read", &path, err))?;
        Ok(Some(LogFile {
            path: path.display().to_string(),
            content,
        }))
    }

    // ---- GitHub ------------------------------------------------------------

    pub async fn resolve_token(&self) -> Result<ResolvedToken> {
        let gh_fallback = self.store.config()?.settings.gh_fallback;
        let env = self.env().await;
        let secrets = Arc::clone(&self.secrets);
        // `security` and `gh` are subprocesses: keep them off the async workers.
        tokio::task::spawn_blocking(move || token::resolve(secrets.as_ref(), &env, gh_fallback))
            .await
            .map_err(join_error)
    }

    pub async fn github(&self) -> Result<GitHub> {
        GitHub::new(&self.api, self.resolve_token().await?.token)
    }

    pub async fn token_status(&self) -> Result<TokenStatus> {
        let resolved = self.resolve_token().await?;
        let gh = GitHub::new(&self.api, resolved.token.clone())?;
        let Some(secret) = resolved.token else {
            return Ok(TokenStatus {
                source: TokenSource::None,
                masked: None,
                login: None,
                rate: gh.rate_limit().await.ok(),
                error: None,
            });
        };
        let (user, rate) = tokio::join!(gh.user(), gh.rate_limit());
        let (login, error) = match user {
            Ok(user) => (Some(user.login), None),
            Err(err) => (None, Some(ErrorInfo::from(&err))),
        };
        Ok(TokenStatus {
            source: resolved.source,
            masked: Some(token::mask(&secret)),
            login,
            rate: rate.ok(),
            error,
        })
    }

    /// Stores a token in the keychain, after checking that GitHub accepts it.
    pub async fn set_token(&self, token: &str, verify: bool) -> Result<TokenStatus> {
        let token = token::validate_token(token)?;
        if verify {
            GitHub::new(&self.api, Some(token.clone()))?.user().await?;
        }
        let secrets = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || secrets.set(&token))
            .await
            .map_err(join_error)??;
        self.token_status().await
    }

    pub async fn clear_token(&self) -> Result<bool> {
        let secrets = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || secrets.delete())
            .await
            .map_err(join_error)?
    }

    pub async fn inspect(&self, repo: &str) -> Result<RepoInspection> {
        inspect::inspect(&self.github().await?, repo).await
    }

    // ---- checks and updates ------------------------------------------------

    pub async fn check(&self, id: &str) -> Result<AppView> {
        let gh = self.github().await?;
        self.check_with(&gh, id).await
    }

    /// Checks several apps (all when `ids` is empty) with bounded concurrency.
    pub async fn check_many(&self, ids: &[String]) -> Result<Vec<(String, Result<AppView>)>> {
        let ids: Vec<String> = if ids.is_empty() {
            self.store
                .config()?
                .apps
                .into_iter()
                .map(|app| app.id)
                .collect()
        } else {
            ids.to_vec()
        };
        let gh = self.github().await?;
        let gh = &gh;
        let results = stream::iter(ids)
            .map(|id| async move {
                let result = self.check_with(gh, &id).await;
                (id, result)
            })
            .buffered(CHECK_CONCURRENCY)
            .collect()
            .await;
        Ok(results)
    }

    async fn check_with(&self, gh: &GitHub, id: &str) -> Result<AppView> {
        let app = self.store.config()?.app(id)?.clone();
        let installed = self.store.state()?.app(id).installed;
        let outcome = check::check(gh, &app, installed.as_ref()).await;
        self.store.update_state(|state| {
            let next = state.with_app(id, |app_state| match &outcome {
                Ok(result) => AppState {
                    last_check: Some(result.clone()),
                    // A good check clears check errors, but not a failed update.
                    last_error: app_state.last_error.filter(|err| err.during != "check"),
                    ..app_state
                },
                Err(err) => AppState {
                    last_error: Some(LastError::from_error(err, "check")),
                    ..app_state
                },
            });
            Ok((next, ()))
        })?;
        outcome?;
        self.get(id)
    }

    /// What `update` would do, without doing it.
    pub async fn plan(&self, id: &str, target: &Target) -> Result<UpdatePlan> {
        let config = self.store.config()?;
        let app = config.app(id)?.clone();
        let gh = self.github().await?;
        let installed = self.store.state()?.app(id).installed;
        let (candidate, _) = self.resolve(&gh, &app, installed.as_ref(), target).await?;
        let asset = match &app.install {
            crate::config::Install::Asset(spec) => {
                let release = gh.release_by_tag(&app.repo_id()?, &candidate.id).await?;
                crate::asset::select(
                    &release.assets,
                    app.kind,
                    spec.pattern.as_deref(),
                    crate::asset::Arch::host(),
                )
                .ok()
                .cloned()
            }
            crate::config::Install::Source(_) => None,
        };
        let steps = update::plan_steps(
            &app,
            &config.settings,
            self.paths(),
            &candidate,
            asset.as_ref(),
        );
        Ok(UpdatePlan {
            app_id: app.id.clone(),
            up_to_date: installed
                .as_ref()
                .is_some_and(|installed| ids_match(&installed.id, &candidate.id)),
            from: installed,
            to: candidate,
            method: app.install.method(),
            steps,
            asset: asset.map(|asset| update::AssetChoice {
                name: asset.name,
                size: asset.size,
                digest: asset.digest,
            }),
        })
    }

    /// Builds or downloads `target` and installs it, streaming progress to `sink`.
    pub async fn update(
        &self,
        id: &str,
        target: &Target,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<UpdateOutcome> {
        let config = self.store.config()?;
        let app = config.app(id)?.clone();
        let settings = config.settings;
        let _lock = AppLock::acquire(&self.paths().app_lock(id), id)?;
        let started = Instant::now();
        let log = RunLog::create(&self.paths().app_log_dir(id), id, sink)?;
        let installed = self.store.state()?.app(id).installed;
        log.stage(
            Stage::Resolve,
            format!("Resolving {} of {}", target.label(), app.name),
        );

        let result = self
            .run_update(&app, &settings, installed.as_ref(), target, &log, cancel)
            .await;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let log_path = Some(log.path().display().to_string());
        match result {
            Ok(Some(to)) => {
                let entry = HistoryEntry {
                    at: Utc::now(),
                    action: HistoryAction::Update,
                    from: installed.as_ref().map(|from| from.id.clone()),
                    to: to.id.clone(),
                    version: to.version.clone(),
                    ok: true,
                    message: None,
                    log_path: log_path.clone(),
                    duration_ms: Some(duration_ms),
                };
                self.store.update_state(|state| {
                    let next = state.with_app(id, |app_state| {
                        app_state.installed_now(to.clone()).record(entry)
                    });
                    Ok((next, ()))
                })?;
                log.stage(Stage::Done, format!("Installed {}", ref_label(&to)));
                Ok(UpdateOutcome {
                    app_id: id.to_owned(),
                    changed: true,
                    from: installed,
                    installed_path: to.path.clone(),
                    to,
                    log_path,
                    duration_ms,
                })
            }
            Ok(None) => {
                let current = installed
                    .clone()
                    .ok_or_else(|| Error::Config("no installed ref".into()))?;
                log.stage(
                    Stage::Done,
                    format!("{} is already installed", ref_label(&current)),
                );
                // Nothing ran: keep `headway log` pointing at the last real build.
                let _ = std::fs::remove_file(log.path());
                Ok(UpdateOutcome {
                    app_id: id.to_owned(),
                    changed: false,
                    from: installed,
                    installed_path: current.path.clone(),
                    to: current,
                    log_path: None,
                    duration_ms,
                })
            }
            Err(err) => {
                log.info(&format!("error: {err}"));
                let entry = HistoryEntry {
                    at: Utc::now(),
                    action: HistoryAction::Update,
                    from: installed.as_ref().map(|from| from.id.clone()),
                    to: match target {
                        Target::Ref(reference) => reference.clone(),
                        Target::Latest | Target::Next => target.label(),
                    },
                    version: None,
                    ok: false,
                    message: Some(err.to_string()),
                    log_path,
                    duration_ms: Some(duration_ms),
                };
                let cancelled = matches!(err, Error::Cancelled);
                self.store.update_state(|state| {
                    let next = state.with_app(id, |app_state| {
                        let app_state = app_state.record(entry);
                        if cancelled {
                            app_state
                        } else {
                            AppState {
                                last_error: Some(LastError::from_error(&err, "update")),
                                ..app_state
                            }
                        }
                    });
                    Ok((next, ()))
                })?;
                Err(err)
            }
        }
    }

    /// `Ok(None)` when the target is already installed.
    async fn run_update(
        &self,
        app: &TrackedApp,
        settings: &Settings,
        installed: Option<&InstalledRef>,
        target: &Target,
        log: &RunLog<'_>,
        cancel: &CancellationToken,
    ) -> Result<Option<InstalledRef>> {
        let token = self.resolve_token().await?;
        let gh = GitHub::new(&self.api, token.token.clone())?;
        let (candidate, check) = self.resolve(&gh, app, installed, target).await?;
        if let Some(check) = check {
            self.store.update_state(|state| {
                let next = state.with_app(&app.id, |app_state| AppState {
                    last_check: Some(check),
                    ..app_state
                });
                Ok((next, ()))
            })?;
        }
        let already = installed.is_some_and(|installed| ids_match(&installed.id, &candidate.id));
        if already && !matches!(target, Target::Ref(_)) {
            return Ok(None);
        }
        let title = Some(candidate.title.as_str()).filter(|title| *title != candidate.id);
        log.info(&format!(
            "Target: {}{}",
            short(&candidate.id),
            title.map(|title| format!(" {title}")).unwrap_or_default()
        ));

        let env = self.env().await;
        let job = Job {
            app,
            settings,
            paths: self.paths(),
            gh: &gh,
            env: &env,
            token: token.token.as_deref(),
            cancel,
            log,
        };
        let done = job.run(&candidate).await?;
        Ok(Some(InstalledRef {
            id: candidate.id.clone(),
            sha: done.sha.or(candidate.sha),
            version: done.version,
            installed_at: Utc::now(),
            path: done.path.as_deref().map(display_path),
        }))
    }

    /// Records `reference` (`latest`, a SHA or a tag) as installed without building it.
    pub async fn mark(&self, id: &str, reference: &str) -> Result<AppView> {
        let app = self.store.config()?.app(id)?.clone();
        let gh = self.github().await?;
        let installed = self.store.state()?.app(id).installed;
        let (candidate, check) = self
            .resolve(&gh, &app, installed.as_ref(), &Target::parse(reference))
            .await?;
        let version = (!matches!(app.track, Track::Commit { .. })).then(|| candidate.id.clone());
        let to = InstalledRef {
            id: candidate.id.clone(),
            sha: candidate.sha.clone(),
            version: version.clone(),
            installed_at: Utc::now(),
            path: None,
        };
        let entry = HistoryEntry {
            at: Utc::now(),
            action: HistoryAction::Mark,
            from: installed.map(|from| from.id),
            to: to.id.clone(),
            version,
            ok: true,
            message: None,
            log_path: None,
            duration_ms: None,
        };
        self.store.update_state(|state| {
            let next = state.with_app(id, |app_state| {
                let app_state = match check {
                    Some(check) => AppState {
                        last_check: Some(check),
                        ..app_state
                    },
                    None => app_state,
                };
                app_state.installed_now(to).record(entry)
            });
            Ok((next, ()))
        })?;
        self.get(id)
    }

    /// Turns a [`Target`] into a concrete candidate. `Latest`/`Next` run a check,
    /// which is returned so callers can cache it.
    async fn resolve(
        &self,
        gh: &GitHub,
        app: &TrackedApp,
        installed: Option<&InstalledRef>,
        target: &Target,
    ) -> Result<(Candidate, Option<CheckResult>)> {
        let reference = match target {
            Target::Latest | Target::Next => {
                let result = check::check(gh, app, installed).await?;
                let stepping = matches!(target, Target::Next)
                    && matches!(
                        result.status,
                        UpdateStatus::UpdateAvailable | UpdateStatus::Diverged
                    );
                let candidate = if stepping {
                    result.next()
                } else {
                    result.latest.as_ref()
                };
                let candidate = candidate.cloned().ok_or_else(|| {
                    Error::RemoteNotFound(format!("no {} found for {}", noun(&app.track), app.repo))
                })?;
                return Ok((candidate, Some(result)));
            }
            Target::Ref(reference) => reference,
        };
        let repo = app.repo_id()?;
        let candidate = match &app.track {
            Track::Commit { .. } => check::commit_candidate(&gh.commit(&repo, reference).await?),
            Track::Release { .. } => {
                check::release_candidate(&gh.release_by_tag(&repo, reference).await?)
            }
            Track::Tag { .. } => {
                let commit = check::commit_candidate(&gh.commit(&repo, reference).await?);
                Candidate {
                    id: reference.clone(),
                    title: reference.clone(),
                    body: None,
                    url: Some(format!("{}/tree/{reference}", repo.html_url())),
                    prerelease: Version::parse(reference)
                        .is_some_and(|version| version.is_prerelease()),
                    ..commit
                }
            }
        };
        Ok((candidate, None))
    }

    // ---- diagnostics -------------------------------------------------------

    pub async fn doctor(&self) -> DoctorReport {
        let env = self.env().await;
        let mut checks = vec![doctor::config_check(&self.store)];
        checks.push(doctor::git_check(&env).await);
        checks.push(match self.token_status().await {
            Ok(status) => doctor::token_check(&status),
            Err(err) => doctor::error_check("github", &err),
        });
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
    let repo = app.repo_id()?.full_name();
    Ok(TrackedApp { repo, ..app })
}

fn noun(track: &Track) -> &'static str {
    match track {
        Track::Commit { .. } => "commit",
        Track::Release { .. } => "release",
        Track::Tag { .. } => "version tag",
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

fn setting_value(settings: &Settings, key: &str) -> String {
    match key {
        "check_interval_minutes" => settings.check_interval_minutes.to_string(),
        "notify" => settings.notify.to_string(),
        "bin_dir" => settings.bin_dir.clone(),
        "apps_dir" => settings.apps_dir.clone(),
        "gh_fallback" => settings.gh_fallback.to_string(),
        _ => String::new(),
    }
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(join_error)`
fn join_error(err: tokio::task::JoinError) -> Error {
    Error::io("background task", std::io::Error::other(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::cli_app;
    use crate::token::MemoryStore;

    fn headway() -> (tempfile::TempDir, Headway) {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(Paths::under(dir.path()))
            .secrets(Arc::new(MemoryStore::default()))
            .env(ShellEnv::current())
            .api("http://127.0.0.1:9")
            .build()
            .unwrap();
        (dir, hw)
    }

    #[test]
    fn add_get_replace_remove() {
        let (_dir, hw) = headway();
        let view = hw.add(cli_app("rg")).unwrap();
        assert_eq!(view.status, AppStatus::Unchecked);
        assert!(!view.busy);
        assert!(matches!(hw.add(cli_app("rg")), Err(Error::AppExists(_))));

        let edited = TrackedApp {
            name: "ripgrep".into(),
            ..cli_app("rg")
        };
        assert_eq!(hw.replace(edited).unwrap().app.name, "ripgrep");
        assert!(matches!(
            hw.replace(cli_app("nope")),
            Err(Error::AppNotFound(_))
        ));

        assert_eq!(hw.list().unwrap().len(), 1);
        hw.remove("rg", true).unwrap();
        assert!(hw.list().unwrap().is_empty());
        assert!(matches!(hw.remove("rg", false), Err(Error::AppNotFound(_))));
    }

    #[test]
    fn changing_the_track_mode_forgets_the_installed_ref() {
        let (_dir, hw) = headway();
        hw.add(cli_app("rg")).unwrap();
        hw.store()
            .update_state(|state| {
                Ok((
                    state.with_app("rg", |app| AppState {
                        installed: Some(InstalledRef {
                            id: "abc1234".into(),
                            sha: Some("abc1234".into()),
                            version: None,
                            installed_at: Utc::now(),
                            path: None,
                        }),
                        ..app
                    }),
                    (),
                ))
            })
            .unwrap();
        let branch_only = TrackedApp {
            track: Track::Commit {
                branch: Some("dev".into()),
            },
            ..cli_app("rg")
        };
        assert!(hw.replace(branch_only).unwrap().installed.is_some());
        let tags = TrackedApp {
            track: Track::Tag {
                prerelease: false,
                tag_filter: None,
            },
            ..cli_app("rg")
        };
        assert!(hw.replace(tags).unwrap().installed.is_none());
    }

    #[test]
    fn busy_apps_cannot_be_removed() {
        let (_dir, hw) = headway();
        hw.add(cli_app("rg")).unwrap();
        let _held = AppLock::acquire(&hw.paths().app_lock("rg"), "rg").unwrap();
        assert_eq!(hw.get("rg").unwrap().status, AppStatus::Busy);
        assert!(matches!(hw.remove("rg", false), Err(Error::Busy(_))));
    }

    #[test]
    fn settings_are_validated() {
        let (_dir, hw) = headway();
        let saved = hw
            .save_settings(&Settings {
                check_interval_minutes: 5,
                ..Settings::default()
            })
            .unwrap();
        assert_eq!(saved.check_interval_minutes, 5);
        assert!(
            hw.save_settings(&Settings {
                bin_dir: "relative".into(),
                ..Settings::default()
            })
            .is_err()
        );
        assert!(!hw.set_setting("notify", "false").unwrap().notify);
        assert!(!hw.settings().unwrap().notify);
    }

    #[test]
    fn repository_urls_are_stored_as_owner_name() {
        let (_dir, hw) = headway();
        let view = hw
            .add(TrackedApp {
                repo: "https://github.com/owner/rg.git".into(),
                ..cli_app("rg")
            })
            .unwrap();
        assert_eq!(view.app.repo, "owner/rg");
    }

    #[test]
    fn unknown_apps_are_reported() {
        let (_dir, hw) = headway();
        assert!(matches!(hw.get("nope"), Err(Error::AppNotFound(_))));
        assert!(matches!(hw.history("nope"), Err(Error::AppNotFound(_))));
        assert!(matches!(hw.last_log("nope"), Err(Error::AppNotFound(_))));
    }
}
