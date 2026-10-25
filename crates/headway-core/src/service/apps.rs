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
        // Repointing an app while it updates would record the old repo's version as installed.
        let current = self.store.config()?.app(&id)?.clone();
        let repointed = current.source != app.source;
        let lock = if repointed {
            Some(AppLock::acquire(&self.paths().app_lock(&id), &id)?)
        } else {
            None
        };
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
        let identity_changed = !previous.source.same_identity(&new.source);
        let check_stale = previous.source != new.source;
        if check_stale {
            self.store.update_state(|state| {
                let next = state.with_app(&id, |app_state| AppState {
                    pull_request: if identity_changed {
                        None
                    } else {
                        app_state.pull_request
                    },
                    retained_installation: if identity_changed {
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
                    },
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
        drop(lock);
        self.get(&id)
    }

    pub fn remove(&self, id: &str, purge: bool) -> Result<RemovedApp> {
        // Held until the end: no update may start while the checkout is being deleted.
        let _lock = AppLock::acquire(&self.paths().app_lock(id), id)?;
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

    /// Records `reference` (`latest`, a SHA or a tag) as installed without building it.
    pub async fn mark(&self, id: &str, reference: &str) -> Result<AppView> {
        self.store.config()?.app(id)?;
        let lock = AppLock::acquire(&self.paths().app_lock(id), id)?;
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
            let _ = self.check(id).await;
        }
        drop(lock);
        self.get(id)
    }
}
