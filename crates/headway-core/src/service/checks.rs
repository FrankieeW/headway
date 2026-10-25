use super::*;

impl Headway {
    // ---- checks and updates ------------------------------------------------

    pub async fn check(&self, id: &str) -> Result<AppView> {
        self.check_with_cancel(id, &CancellationToken::new()).await
    }

    pub async fn check_with_cancel(&self, id: &str, cancel: &CancellationToken) -> Result<AppView> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let app = self.store.config()?.app(id)?.clone();
        let lock = if app.source.detects_installed() || crate::recovery::pending(self.paths(), &app)
        {
            Some(AppLock::acquire_app_async(self.paths(), id).await?)
        } else {
            None
        };
        let app = self.store.config()?.app(id)?.clone();
        if lock.is_some() {
            crate::recovery::reconcile(self.paths(), &app)?;
        }
        self.check_unlocked(&app, cancel).await?;
        drop(lock);
        self.get(id)
    }

    /// Checks several apps (all when `ids` is empty) with bounded concurrency.
    pub async fn check_many(&self, ids: &[String]) -> Result<Vec<(String, Result<AppView>)>> {
        self.check_many_with_cancel(ids, &CancellationToken::new())
            .await
    }

    pub async fn check_many_with_cancel(
        &self,
        ids: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<(String, Result<AppView>)>> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let ids: Vec<String> = if ids.is_empty() {
            self.store
                .config()?
                .apps
                .into_iter()
                .map(|app| app.id)
                .collect()
        } else {
            let mut seen = std::collections::HashSet::new();
            ids.iter()
                .filter(|id| seen.insert((*id).clone()))
                .cloned()
                .collect()
        };
        let results = stream::iter(ids)
            .map(|id| async move {
                let result = self.check_with_cancel(&id, cancel).await;
                (id, result)
            })
            .buffered(CHECK_CONCURRENCY)
            .collect()
            .await;
        Ok(results)
    }

    pub async fn check_summary(&self, ids: &[String]) -> Result<CheckSummary> {
        Ok(CheckSummary::from_results(&self.check_many(ids).await?))
    }

    pub(super) async fn check_unlocked(
        &self,
        app: &TrackedApp,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let id = &app.id;
        let gh = self.github_for(app).await?;
        let env = self.env().await;
        let installed = self.store.state()?.app(id).installed;
        let checked_against = installed.as_ref().map(|installed| installed.id.clone());
        let outcome = super::presence::check(
            &Probe {
                gh: &gh,
                env: &env,
                cancel,
            },
            app,
            installed.as_ref(),
        )
        .await;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.store.config()?.app(id)?.source != app.source {
            return Err(Error::ConfigChanged(id.clone()));
        }
        self.store
            .update_state_for_source(id, &app.source, |state| {
                let next = state.with_app(id, |app_state| match &outcome {
                    Ok(checked) => {
                        let app_state = sync_installed(app_state, &checked.installed);
                        AppState {
                            // An update may have finished while GitHub was asked: re-center on it.
                            last_check: Some(match app_state.installed.as_ref() {
                                Some(now)
                                    if !app.source.detects_installed()
                                        && Some(&now.id) != checked_against.as_ref() =>
                                {
                                    checked.result.clone().rebased_on(&now.id)
                                }
                                _ => checked.result.clone(),
                            }),
                            // A good check clears check errors, but not a failed update.
                            last_error: app_state.last_error.filter(|err| err.during != "check"),
                            ..app_state
                        }
                    }
                    Err(err) => AppState {
                        last_error: Some(LastError::from_error(err, "check")),
                        ..app_state
                    },
                });
                Ok((next, ()))
            })?;
        outcome?;
        Ok(())
    }

    pub(super) async fn refresh_for_update(
        &self,
        app: &TrackedApp,
        target: &Target,
        cancel: &CancellationToken,
    ) -> Result<()> {
        if let (Source::Npm(source), Target::Ref(_)) = (&app.source, target) {
            let version =
                crate::npm::installed_version(cancel, &self.env().await, &source.package).await?;
            self.store.update_state(|state| {
                Ok((
                    state.with_app(&app.id, |state| {
                        sync_installed(state, &InstalledSync::Detected(version))
                    }),
                    (),
                ))
            })
        } else {
            self.check_unlocked(app, cancel).await
        }
    }
}

pub(super) fn sync_installed(state: AppState, sync: &InstalledSync) -> AppState {
    let InstalledSync::Detected(version) = sync else {
        return state;
    };
    let installed = version.as_ref().map(|version| {
        state
            .installed
            .as_ref()
            .filter(|current| &current.id == version)
            .cloned()
            .unwrap_or_else(|| InstalledRef {
                id: version.clone(),
                version: Some(version.clone()),
                sha: None,
                path: None,
                installed_at: Utc::now(),
            })
    });
    AppState { installed, ..state }
}
