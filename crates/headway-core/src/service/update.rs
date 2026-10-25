use super::update_method::run_recipe;
use super::*;

use crate::revision_install::RevisionMethod;
use crate::update::UpdateRequest;

impl Headway {
    /// Builds or downloads `target` and installs it, streaming progress to `sink`.
    pub async fn update(
        &self,
        id: &str,
        target: &Target,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<UpdateOutcome> {
        self.update_with(
            id,
            &UpdateRequest {
                target: target.clone(),
                method: RevisionMethod::Configured,
            },
            sink,
            cancel,
        )
        .await
    }

    /// Builds or downloads the requested target and installs it.
    ///
    /// `request.method` forces the install method for this run only: a
    /// commit-tracked app can be updated from the release that published the
    /// commit, or built from source, without changing its saved recipe.
    pub async fn update_with(
        &self,
        id: &str,
        request: &UpdateRequest,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<UpdateOutcome> {
        let target = &request.target;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.store.config()?.app(id)?;
        let _lock = AppLock::acquire_app_async(self.paths(), id).await?;
        let config = self.store.config()?;
        let app = config.app(id)?.clone();
        crate::recovery::reconcile(self.paths(), &app)?;
        // A forced method is checked before anything is resolved, logged or recorded.
        let baseline = super::presence::retained(&app, self.store.state()?.app(id).installed)?;
        let recipe = run_recipe(&app, request.method, baseline.as_ref())?;
        let settings = config.settings;
        if app.source.detects_installed() {
            self.refresh_for_update(&app, target, cancel).await?;
        }
        let started = Instant::now();
        let log = RunLog::create(&self.paths().app_log_dir(id), id, sink)?;
        let installed = super::presence::retained(&app, self.store.state()?.app(id).installed)?;
        log.stage(
            Stage::Resolve,
            format!("Resolving {} of {}", target.label(), app.name),
        );

        let attempt = Attempt {
            app: &app,
            recipe: &recipe,
            settings: &settings,
            installed: installed.as_ref(),
            target,
            log: &log,
            cancel,
        };
        let mut resolved = None;
        let result = self.run_update(&attempt, &mut resolved).await;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let log_path = Some(log.path().display().to_string());
        match result {
            Ok(Some(to)) => {
                let changed = to.id == crate::script::LATEST
                    || installed.as_ref().is_none_or(|from| from.id != to.id);
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
                // The journal names the recipe's destination, so it is committed
                // against the recipe the run installed with.
                crate::recovery::commit(self.paths(), &recipe)?;
                log.stage(Stage::Done, format!("Installed {}", ref_label(&to)));
                if app.source.detects_installed() {
                    if let Err(err) = self.check_unlocked(&app, cancel).await {
                        log.info(&format!(
                            "Could not refresh the status after installing: {err}"
                        ));
                    }
                } else if matches!(target, Target::Ref(_)) {
                    // A rollback or a pick from history lands outside the cached window:
                    // recompute what is newer than it. Failures show up as a check error.
                    let _ = self.check(id).await;
                }
                Ok(UpdateOutcome {
                    app_id: id.to_owned(),
                    changed,
                    from: installed,
                    installed_path: to.path.clone(),
                    to,
                    log_path,
                    duration_ms,
                    warnings: log.warnings(),
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
                    warnings: log.warnings(),
                })
            }
            Err(err) => {
                log.info(&format!("error: {err}"));
                let to = resolved.unwrap_or_else(|| match target {
                    Target::Ref(reference) => reference.clone(),
                    Target::Latest => "latest".into(),
                    Target::Next => "next".into(),
                });
                if let Err(record_error) = self.record_failure(&attempt, to, &err, duration_ms) {
                    log.info(&format!(
                        "Could not record the update failure: {record_error}"
                    ));
                }
                Err(err)
            }
        }
    }

    /// `Ok(None)` when the target is already installed. `resolved` receives the id of the
    /// commit or tag being installed as soon as it is known.
    async fn run_update(
        &self,
        attempt: &Attempt<'_>,
        resolved: &mut Option<String>,
    ) -> Result<Option<InstalledRef>> {
        let Attempt {
            app,
            recipe,
            settings,
            installed,
            target,
            log,
            cancel,
        } = *attempt;
        let token = if app.github().is_some() {
            self.resolve_token().await?.token
        } else {
            None
        };
        let gh = GitHub::new(&self.api, token.clone())?;
        let (candidate, check) = self.resolve(&gh, app, installed, target, cancel).await?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let current = matches!(app.source, Source::Npm(_))
            && check
                .as_ref()
                .is_some_and(|check| check.result.status == UpdateStatus::UpToDate);
        *resolved = Some(candidate.id.clone());
        if let Some(check) = check {
            self.store.update_state(|state| {
                let next = state.with_app(&app.id, |app_state| AppState {
                    last_check: Some(check.result),
                    ..app_state
                });
                Ok((next, ()))
            })?;
        }
        let already =
            installed.is_some_and(|installed| target_matches(app, &installed.id, &candidate.id));
        if (already || current)
            && !matches!(target, Target::Ref(_))
            && candidate.id != crate::script::LATEST
        {
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
            app: recipe,
            settings,
            paths: self.paths(),
            gh: &gh,
            env: &env,
            token: token.as_deref(),
            cancel,
            log,
        };
        let done = job.run(&candidate).await?;
        Ok(Some(InstalledRef {
            id: if app.source.detects_installed() {
                done.version
                    .clone()
                    .unwrap_or_else(|| crate::script::LATEST.into())
            } else {
                candidate.id.clone()
            },
            sha: done.sha.or(candidate.sha),
            version: done.version,
            installed_at: Utc::now(),
            path: done.path.as_deref().map(display_path),
        }))
    }

    fn record_failure(
        &self,
        attempt: &Attempt<'_>,
        to: String,
        err: &Error,
        duration_ms: u64,
    ) -> Result<()> {
        let entry = HistoryEntry {
            at: Utc::now(),
            action: HistoryAction::Update,
            from: attempt.installed.map(|from| from.id.clone()),
            to,
            version: None,
            ok: false,
            message: Some(err.to_string()),
            log_path: Some(attempt.log.path().display().to_string()),
            duration_ms: Some(duration_ms),
        };
        self.store.update_state(|state| {
            let next = state.with_app(&attempt.app.id, |app_state| {
                let app_state = AppState {
                    last_check: if matches!(err, Error::Cancelled) {
                        app_state.last_check.clone()
                    } else {
                        None
                    },
                    ..app_state.record(entry)
                };
                if matches!(err, Error::Cancelled) {
                    app_state
                } else {
                    AppState {
                        last_error: Some(LastError::from_error(err, "update")),
                        ..app_state
                    }
                }
            });
            Ok((next, ()))
        })
    }
}
