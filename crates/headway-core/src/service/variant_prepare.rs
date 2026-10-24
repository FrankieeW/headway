//! Preparation of an independent install without changing configuration or payloads.
use super::revision_prepare::PreparedRevision;
use super::variants::same_destination;
use crate::{AppKind, Error, Headway, Install, Result, Source, Track, VariantInfo, VariantRequest};
impl Headway {
    pub(super) async fn prepare_variant(
        &self,
        parent_id: &str,
        request: &VariantRequest,
    ) -> Result<(PreparedRevision, crate::Settings)> {
        crate::repo::validate_app_id(&request.id)?;
        if request.id == parent_id {
            return Err(Error::InvalidInput(
                "independent version requires a different app id".into(),
            ));
        }
        let config = self.store().config()?;
        let parent = config.app(parent_id)?.clone();
        if config.apps.iter().any(|app| app.id == request.id) {
            return Err(Error::AppExists(request.id.clone()));
        }
        let mut prepared = self
            .prepare_revision(parent.clone(), &request.revision)
            .await?;
        let base = match parent.kind {
            AppKind::App => {
                crate::paths::expand_tilde(&config.settings.apps_dir).join("Headway Versions")
            }
            AppKind::Cli => crate::paths::expand_tilde(&config.settings.bin_dir),
        };
        let dest = base.join(if parent.kind == AppKind::App {
            format!("{}.app", request.id)
        } else {
            request.id.clone()
        });
        match std::fs::symlink_metadata(&dest) {
            Ok(_) => {
                return Err(Error::InvalidInput(format!(
                    "independent destination already exists: {}",
                    dest.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(Error::io_at(
                    "inspect independent destination",
                    &dest,
                    error,
                ));
            }
        }
        let state = self.store().state()?;
        for app in &config.apps {
            let configured = app.github().and_then(|g| match &g.install {
                Install::Source(s) => s.dest.as_deref(),
                Install::Asset(s) => s.dest.as_deref(),
            });
            let installed = state.app(&app.id).installed;
            if configured
                .or_else(|| installed.as_ref().and_then(|i| i.path.as_deref()))
                .is_some_and(|path| same_destination(&crate::paths::expand_tilde(path), &dest))
            {
                return Err(Error::InvalidInput(
                    "independent destination belongs to another tracked app".into(),
                ));
            }
        }
        let branch = match &parent.require_github("independent versions")?.track {
            Track::Commit { branch } => branch.clone(),
            _ => None,
        };
        prepared.app.id.clone_from(&request.id);
        prepared.app.name = request
            .name
            .clone()
            .unwrap_or_else(|| format!("{} ({})", parent.name, &prepared.options.commit[..7]));
        prepared.app.auto_update = false;
        prepared.app.update_policy.mode = crate::AutoUpdateMode::Notify;
        let Source::Github(github) = &mut prepared.app.source else {
            return Err(Error::InvalidInput(
                "independent versions require GitHub".into(),
            ));
        };
        if matches!(&github.install,Install::Source(source) if source.artifact.is_none()) {
            return Err(Error::InvalidInput("independent versions require a managed artifact; self-installing build commands cannot choose a separate destination".into()));
        }
        match &mut github.install {
            Install::Source(source) => source.dest = Some(dest.display().to_string()),
            Install::Asset(asset) => asset.dest = Some(dest.display().to_string()),
        }
        github.track = Track::Commit {
            branch: Some(prepared.options.commit.clone()),
        };
        github.options.variant = Some(VariantInfo {
            parent_id: parent_id.into(),
            reference: prepared.options.commit.clone(),
            release_tag: prepared.options.release_tag.clone(),
            branch,
        });
        Ok((prepared, config.settings))
    }
}
