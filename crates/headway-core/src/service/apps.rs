use super::*;

impl Headway {
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
        let busy = lock::app_is_locked(self.paths(), &app.id);
        let mut app_state = state.app(&app.id);
        if !busy && crate::recovery::pending(self.paths(), app) {
            // Reading never rolls payloads back. Do not display an uncertain journal
            // baseline as installed/up-to-date until a check reconciles under its lock.
            app_state.installed = None;
            app_state.last_check = None;
            app_state.pull_request = None;
            app_state.last_error = Some(LastError::from_error(
                &Error::RecoveryPending(app.id.clone()),
                "recovery",
            ));
        }
        AppView::new(app.clone(), app_state, busy)
    }

    /// Exactly the add validation, without writing config or state.
    pub fn validate_add(&self, app: TrackedApp) -> Result<TrackedApp> {
        let app = normalized(app).map_err(candidate_error)?;
        let mut config = self.store.config()?;
        if config.apps.iter().any(|other| other.id == app.id) {
            return Err(Error::AppExists(app.id));
        }
        if crate::recovery::pending(self.paths(), &app) {
            return Err(Error::RecoveryPending(app.id));
        }
        config.apps.push(app.clone());
        config.validate().map_err(candidate_error)?;
        self.validate_app_destination(&config, &app)
            .map_err(candidate_error)?;
        Ok(app)
    }

    /// Exactly the replacement validation, without writing config or state.
    pub fn validate_replace(&self, app: TrackedApp) -> Result<TrackedApp> {
        let app = normalized(app).map_err(candidate_error)?;
        let mut config = self.store.config()?;
        let index = config
            .apps
            .iter()
            .position(|other| other.id == app.id)
            .ok_or_else(|| Error::AppNotFound(app.id.clone()))?;
        config.apps[index] = app.clone();
        config.validate().map_err(candidate_error)?;
        self.validate_app_destination(&config, &app)
            .map_err(candidate_error)?;
        Ok(app)
    }

    pub(super) fn validate_app_destination(&self, config: &Config, app: &TrackedApp) -> Result<()> {
        let Some(dest) = crate::config::destination(app, &config.settings) else {
            return Ok(());
        };
        let state = self.store.state()?;
        for other in config.apps.iter().filter(|other| other.id != app.id) {
            let other_state = state.app(&other.id);
            let destinations = [
                crate::config::destination(other, &config.settings),
                other_state
                    .installed
                    .and_then(|installed| installed.path)
                    .map(|path| crate::paths::expand_tilde(&path)),
                other_state
                    .retained_installation
                    .and_then(|retained| retained.installed.path)
                    .map(|path| crate::paths::expand_tilde(&path)),
            ];
            if destinations
                .into_iter()
                .flatten()
                .any(|other_dest| super::variants::same_destination(&dest, &other_dest))
            {
                return Err(Error::Config(format!(
                    "destination `{}` belongs to app `{}`",
                    dest.display(),
                    other.id
                )));
            }
        }
        Ok(())
    }

    pub fn add(&self, app: TrackedApp) -> Result<AppView> {
        let app = self.validate_add(app)?;
        let id = app.id.clone();
        let lock = AppLock::acquire_app(self.paths(), &id)?;
        if crate::recovery::pending(self.paths(), &app) {
            return Err(Error::RecoveryPending(id));
        }
        self.store.register(|mut config, state| {
            if config.apps.iter().any(|existing| existing.id == app.id) {
                return Err(Error::AppExists(app.id));
            }
            config.apps.push(app);
            config.validate().map_err(candidate_error)?;
            Ok((config, state.without_app(&id), ()))
        })?;
        drop(lock);
        self.get(&id)
    }

    /// Replaces an app's config. Changing what is tracked forgets the installed ref,
    /// since it no longer describes the same thing.
    pub fn replace(&self, app: TrackedApp) -> Result<AppView> {
        let app = self.validate_replace(app)?;
        let id = app.id.clone();
        // Repointing an app while it updates would record the old repo's version as installed.
        let current = self.store.config()?.app(&id)?.clone();
        let repointed = current.source != app.source;
        let lock = if repointed {
            Some(AppLock::acquire_app(self.paths(), &id)?)
        } else {
            None
        };
        if lock.is_some() {
            let current = self.store.config()?.app(&id)?.clone();
            crate::recovery::reconcile(self.paths(), &current)?;
        }
        self.store.register(|mut config, state| {
            let index = config
                .apps
                .iter()
                .position(|existing| existing.id == id)
                .ok_or_else(|| Error::AppNotFound(id.clone()))?;
            let previous = &config.apps[index];
            // A policy-only edit deliberately need not interrupt an ongoing update.
            // If a source edit raced its initial read, retry with the app lock instead.
            if lock.is_none() && previous.source != app.source {
                return Err(Error::ConfigChanged(id.clone()));
            }
            let identity_changed = !previous.source.same_identity(&app.source);
            let state = if previous.source == app.source {
                state
            } else {
                state.with_app(&id, |app_state| {
                    let retained_installation = if identity_changed {
                        previous
                            .github()
                            .and_then(|github| {
                                app_state.installed.clone().map(|installed| {
                                    crate::state::RetainedInstallation {
                                        repo: github.repo.clone(),
                                        installed,
                                    }
                                })
                            })
                            .or(app_state.retained_installation)
                    } else {
                        app_state.retained_installation
                    };
                    AppState {
                        retained_installation,
                        installed: if identity_changed {
                            None
                        } else {
                            app_state.installed
                        },
                        pull_request: if identity_changed {
                            None
                        } else {
                            app_state.pull_request
                        },
                        last_check: None,
                        last_error: None,
                        ..app_state
                    }
                })
            };
            config.apps[index] = app;
            config.validate().map_err(candidate_error)?;
            // State invalidation is durable before the new source becomes visible.
            Ok((config, state, ()))
        })?;
        drop(lock);
        self.get(&id)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<RemovedApp> {
        self.store.config()?.app(id)?;
        // Held until the end: no update may start while the checkout is being deleted.
        let _lock = AppLock::acquire_app(self.paths(), id)?;
        let current = self.store.config()?.app(id)?.clone();
        crate::recovery::reconcile(self.paths(), &current)?;
        if purge {
            let config = self.store.config()?;
            crate::source_storage::purge(self.paths(), config.app(id)?)?;
            crate::recovery::purge(self.paths(), id)?;
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
            let dir = self.paths().app_log_dir(id);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|err| Error::io_at("remove", &dir, err))?;
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
        settings.validate()?;
        let validated = settings.clone();
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

    /// Records `reference` (`latest`, a SHA or a tag) as installed without building it.
    pub async fn mark(&self, id: &str, reference: &str) -> Result<AppView> {
        self.store.config()?.app(id)?;
        let lock = AppLock::acquire_app_async(self.paths(), id).await?;
        let app = self.store.config()?.app(id)?.clone();
        let github =
            app.require_github("mark (other sources detect installed versions from the system)")?;
        let gh = self.github().await?;
        let installed = self.store.state()?.app(id).installed;
        let (candidate, check) = self
            .resolve(
                &gh,
                &app,
                installed.as_ref(),
                &Target::parse(reference),
                &CancellationToken::new(),
            )
            .await?;
        let explicit = check.is_none();
        let version = (!matches!(github.track, Track::Commit { .. })).then(|| candidate.id.clone());
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
                        last_check: Some(check.result),
                        ..app_state
                    },
                    None => app_state,
                };
                app_state.installed_now(to).record(entry)
            });
            Ok((next, ()))
        })?;
        if explicit {
            // The cache knows nothing about an arbitrary ref: recompute what is newer.
            if let Err(err) = self.check(id).await {
                self.store.update_state(|state| {
                    Ok((
                        state.with_app(id, |state| AppState {
                            last_error: Some(LastError::from_error(&err, "check")),
                            ..state
                        }),
                        (),
                    ))
                })?;
            }
        }
        drop(lock);
        self.get(id)
    }
}
