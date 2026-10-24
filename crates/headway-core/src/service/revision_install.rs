use crate::state::{AppState, HistoryAction, LastError};
use crate::update::{Job, RunLog};
use crate::{
    CancellationToken, Error, EventSink, Headway, HistoryEntry, InstalledRef, Result,
    RevisionRequest, Stage, UpdateOutcome, UpdatePlan,
};
use std::time::Instant;

impl Headway {
    pub async fn plan_revision(&self, id: &str, request: &RevisionRequest) -> Result<UpdatePlan> {
        let config = self.store().config()?;
        let prepared = self
            .prepare_revision(config.app(id)?.clone(), request)
            .await?;
        let from =
            super::presence::retained(&prepared.app, self.store().state()?.app(id).installed)?;
        Ok(UpdatePlan {
            app_id: id.into(),
            up_to_date: false,
            reinstall: from
                .as_ref()
                .is_some_and(|i| i.sha.as_deref() == Some(&prepared.options.commit)),
            from,
            to: prepared.options.candidate,
            method: prepared.app.source.method(),
            steps: crate::update::plan_steps(
                &prepared.app,
                &config.settings,
                self.paths(),
                &prepared.job_target,
                prepared.asset.as_ref(),
            ),
            asset: prepared.asset.map(|a| crate::update::AssetChoice {
                name: a.name,
                size: a.size,
                digest: a.digest,
            }),
        })
    }

    pub async fn install_revision(
        &self,
        id: &str,
        request: &RevisionRequest,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<UpdateOutcome> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.store().config()?.app(id)?;
        let _lock = crate::lock::AppLock::acquire(&self.paths().app_lock(id), id)?;
        let config = self.store().config()?;
        let prepared = self
            .prepare_revision(config.app(id)?.clone(), request)
            .await?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let from =
            super::presence::retained(&prepared.app, self.store().state()?.app(id).installed)?;
        let started = Instant::now();
        let log = RunLog::create(&self.paths().app_log_dir(id), id, sink)?;
        log.stage(
            Stage::Resolve,
            format!(
                "Installing exact commit {} using {}",
                prepared.options.commit,
                prepared.app.source.method()
            ),
        );
        let gh = self.github_for(&prepared.app).await?;
        let token = self.resolve_token().await?.token;
        let env = self.env().await;
        self.pin_variant(id, &prepared)?;
        let result = Job {
            app: &prepared.app,
            settings: &config.settings,
            paths: self.paths(),
            gh: &gh,
            env: &env,
            token: token.as_deref(),
            cancel,
            log: &log,
        }
        .run(&prepared.job_target)
        .await;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let log_path = Some(log.path().display().to_string());
        let installed = result.as_ref().ok().map(|done| InstalledRef {
            id: prepared.installed_id.clone(),
            sha: Some(prepared.options.commit.clone()),
            version: done.version.clone(),
            installed_at: chrono::Utc::now(),
            path: done.path.as_deref().map(crate::paths::display_path),
        });
        let entry = HistoryEntry {
            at: chrono::Utc::now(),
            action: HistoryAction::Update,
            from: from.as_ref().map(|i| i.id.clone()),
            to: prepared.installed_id,
            version: installed.as_ref().and_then(|i| i.version.clone()),
            ok: result.is_ok(),
            message: result.as_ref().err().map(ToString::to_string),
            log_path: log_path.clone(),
            duration_ms: Some(duration_ms),
        };
        self.store().update_state(|state| {
            Ok((
                state.with_app(id, |state| {
                    let state = state.record(entry);
                    match (&result, &installed) {
                        (Ok(_), Some(installed)) => AppState {
                            installed: Some(installed.clone()),
                            last_check: None,
                            last_error: None,
                            ..state
                        },
                        (Err(error), _) => AppState {
                            last_error: Some(LastError::from_error(error, "update")),
                            ..state
                        },
                        _ => state,
                    }
                }),
                (),
            ))
        })?;
        result?;
        let to = installed
            .ok_or_else(|| Error::Config("installation completed without a revision".into()))?;
        log.stage(Stage::Done, format!("Installed {}", to.id));
        Ok(UpdateOutcome {
            app_id: id.into(),
            changed: true,
            from,
            installed_path: to.path.clone(),
            to,
            log_path,
            duration_ms,
        })
    }
    // Persist the variant's requested pin before running a build, so a config
    // write failure cannot leave a newly installed payload attributed to an old ref.
    fn pin_variant(
        &self,
        id: &str,
        prepared: &super::revision_prepare::PreparedRevision,
    ) -> Result<()> {
        if prepared
            .app
            .require_github("commit installation")?
            .options
            .variant
            .is_none()
        {
            return Ok(());
        }
        self.store().update_config(|mut config| {
            let app = config
                .apps
                .iter_mut()
                .find(|app| app.id == id)
                .ok_or_else(|| Error::AppNotFound(id.into()))?;
            if let crate::Source::Github(github) = &mut app.source {
                github.track = crate::Track::Commit {
                    branch: Some(prepared.options.commit.clone()),
                };
                github.install = prepared
                    .app
                    .require_github("commit installation")?
                    .install
                    .clone();
                if let Some(variant) = &mut github.options.variant {
                    variant.reference.clone_from(&prepared.options.commit);
                    variant
                        .release_tag
                        .clone_from(&prepared.options.release_tag);
                }
            }
            Ok((config, ()))
        })
    }
}

#[cfg(test)]
#[path = "revision_install_tests.rs"]
mod tests;
