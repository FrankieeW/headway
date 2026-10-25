use super::*;
use crate::recovery::{BackupEntry, Cache, Replacement, RestoreOutcome, component, verification};

impl Headway {
    /// Lists the two retained prior payloads. Unsupported providers return an error.
    pub fn list_backups(&self, id: &str) -> Result<Vec<BackupEntry>> {
        component(id)?;
        let _lock = AppLock::acquire_app(self.paths(), id)?;
        let config = self.store.config()?;
        let app = config.app(id)?;
        verification(app)?;
        Cache::new(self.paths(), app)?.list()
    }

    /// Restores a cached payload without fetching or rebuilding it, then verifies it.
    pub async fn restore_backup(
        &self,
        id: &str,
        backup_id: &str,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<RestoreOutcome> {
        component(id)?;
        component(backup_id)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let _lock = AppLock::acquire_app_async(self.paths(), id).await?;
        let config = self.store.config()?;
        let app = config.app(id)?;
        crate::recovery::reconcile(self.paths(), app)?;
        verification(app)?;
        let (backup, dest) = Cache::new(self.paths(), app)?.load(backup_id)?;
        let current = self.store.state()?.app(id).installed;
        let configured = match &app.require_github("recovery")?.install {
            crate::Install::Source(source) => source.dest.as_deref(),
            crate::Install::Asset(asset) => asset.dest.as_deref(),
        };
        let artifact = std::path::Path::new(&backup.artifact_path);
        let name = artifact
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::InvalidInput("invalid backup destination".into()))?;
        let expected = crate::install::resolve_dest(configured, app.kind, name, &config.settings);
        let current_path = current
            .as_ref()
            .and_then(|installed| installed.path.as_deref());
        if dest != expected
            || (configured.is_none()
                && current_path.is_some_and(|path| crate::paths::expand_tilde(path) != dest))
        {
            return Err(Error::InvalidInput(
                "backup destination does not match the current install configuration".into(),
            ));
        }
        let log = RunLog::create(&self.paths().app_log_dir(id), id, sink)?;
        let started = Instant::now();
        let env = self.env().await;
        let result = Replacement {
            app,
            paths: self.paths(),
            env: &env,
            log: &log,
            cancel,
        }
        .place(
            std::path::Path::new(&backup.artifact_path),
            backup.installed.as_ref(),
            &dest,
        )
        .await;
        let installed = backup.installed.map(|old| InstalledRef {
            installed_at: Utc::now(),
            path: Some(display_path(&dest)),
            ..old
        });
        let entry = HistoryEntry {
            at: Utc::now(),
            action: HistoryAction::Update,
            from: current.as_ref().map(|old| old.id.clone()),
            to: installed
                .as_ref()
                .map_or_else(|| format!("backup:{backup_id}"), |old| old.id.clone()),
            version: installed.as_ref().and_then(|old| old.version.clone()),
            ok: result.is_ok(),
            message: Some(match &result {
                Ok(()) => format!("Restored retained backup {backup_id}"),
                Err(error) => format!("Restore {backup_id} failed: {error}"),
            }),
            log_path: Some(log.path().display().to_string()),
            duration_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
        };
        self.store.update_state(|state| {
            let next = state.with_app(id, |state| {
                let state = state.record(entry);
                match &result {
                    Ok(()) => AppState {
                        retained_installation: None,
                        pull_request: None,
                        installed: installed.clone(),
                        last_check: None,
                        last_error: None,
                        ..state
                    },
                    Err(error) => AppState {
                        last_error: Some(LastError::from_error(error, "update")),
                        ..state
                    },
                }
            });
            Ok((next, ()))
        })?;
        result?;
        crate::recovery::commit(self.paths(), app)?;
        log.stage(Stage::Done, format!("Restored backup {backup_id}"));
        Ok(RestoreOutcome {
            app_id: id.into(),
            backup_id: backup_id.into(),
            installed,
            restored_path: display_path(&dest),
        })
    }
}
