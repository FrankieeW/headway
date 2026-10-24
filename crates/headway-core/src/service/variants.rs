//! Separate installed payloads with the application's existing shared data behavior.
use crate::{
    CancellationToken, Error, EventSink, Headway, Install, Result, UpdateOutcome, VariantRequest,
};
use std::path::{Path, PathBuf};

impl Headway {
    /// Read-only preview including the independent destination, without registering an app.
    pub async fn plan_variant(
        &self,
        parent_id: &str,
        request: &VariantRequest,
    ) -> Result<crate::UpdatePlan> {
        let (prepared, settings) = self.prepare_variant(parent_id, request).await?;
        Ok(crate::UpdatePlan {
            app_id: request.id.clone(),
            from: None,
            to: prepared.options.candidate,
            up_to_date: false,
            reinstall: false,
            method: prepared.app.source.method(),
            steps: crate::update::plan_steps(
                &prepared.app,
                &settings,
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

    /// Registers a pinned independent install and installs it without replacing its parent.
    /// Failed installs remain registered for inspection and an explicit retry.
    pub async fn install_variant(
        &self,
        parent_id: &str,
        request: &VariantRequest,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<UpdateOutcome> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let lock = crate::lock::AppLock::acquire(&self.paths().app_lock(parent_id), parent_id)?;
        let (prepared, _) = self.prepare_variant(parent_id, request).await?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.add(prepared.app)?;
        drop(lock);
        let exact = crate::RevisionRequest {
            reference: prepared.options.commit,
            ..request.revision.clone()
        };
        self.install_revision(&request.id, &exact, sink, cancel)
            .await
    }

    /// Internal startup guard: independent Headway instances must not take over the main CLI.
    pub fn running_is_variant(&self) -> Result<bool> {
        Ok(self.running_variant_id()?.is_some())
    }

    pub(crate) fn running_variant_id(&self) -> Result<Option<String>> {
        let exe = std::env::current_exe().map_err(|e| Error::io("current executable", e))?;
        let exe = exe.canonicalize().unwrap_or(exe);
        let config = self.store().config()?;
        let state = self.store().state()?;
        Ok(config.apps.iter().find_map(|app| {
            app.github().and_then(|g| {
                g.options.variant.as_ref()?;
                let installed = state.app(&app.id).installed;
                let configured = match &g.install {
                    Install::Source(s) => s.dest.as_deref(),
                    Install::Asset(s) => s.dest.as_deref(),
                };
                let matches = configured
                    .or_else(|| installed.as_ref().and_then(|i| i.path.as_deref()))
                    .is_some_and(|path| {
                        let path = crate::paths::expand_tilde(path);
                        exe.starts_with(path.canonicalize().unwrap_or(path))
                    });
                matches.then(|| app.id.clone())
            })
        }))
    }
}

pub(super) fn same_destination(left: &Path, right: &Path) -> bool {
    fn resolved(path: &Path) -> PathBuf {
        path.canonicalize()
            .or_else(|_| {
                path.parent()
                    .ok_or_else(|| std::io::Error::other("no parent"))?
                    .canonicalize()
                    .map(|parent| parent.join(path.file_name().unwrap_or_default()))
            })
            .unwrap_or_else(|_| path.to_owned())
    }
    resolved(left) == resolved(right)
}

#[cfg(test)]
#[path = "variants_tests.rs"]
mod tests;
