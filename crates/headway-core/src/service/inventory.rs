use super::{AppView, Headway, normalized};
use crate::inventory::InventoryReport;
use crate::recipes::AppRecipe;
use crate::{Error, Result, TrackedApp};

impl Headway {
    /// Lists installed brew/npm packages without checking upstream or installing anything.
    ///
    /// # Errors
    /// Returns an error for unsupported sources or unreadable configuration.
    pub async fn discover_apps(&self, sources: &[String]) -> Result<InventoryReport> {
        let config = self.store.config()?;
        let mut report = crate::inventory::discover(sources, &self.env().await).await?;
        let discovered: Vec<_> = report
            .candidates
            .iter()
            .map(|item| item.app.clone())
            .collect();
        for (index, candidate) in report.candidates.iter_mut().enumerate() {
            for existing in &config.apps {
                if existing.source.same_identity(&candidate.app.source) {
                    candidate.already_tracked = true;
                    candidate
                        .conflicts
                        .push(format!("source already tracked by `{}`", existing.id));
                } else if existing.id == candidate.app.id
                    || existing.name.eq_ignore_ascii_case(&candidate.app.name)
                {
                    candidate.conflicts.push(format!(
                        "name or id conflicts with tracked app `{}`",
                        existing.id
                    ));
                }
            }
            for (other_index, other) in discovered.iter().enumerate() {
                if other_index != index
                    && (other.source.same_identity(&candidate.app.source)
                        || other.name.eq_ignore_ascii_case(&candidate.app.name))
                {
                    candidate.conflicts.push(format!(
                        "source or name conflicts with discovered app `{}`",
                        other.id
                    ));
                }
            }
        }
        Ok(report)
    }

    /// Validates selections against current config without writing or installing anything.
    /// Returns normalized payloads with auto-update disabled, ready for review.
    ///
    /// # Errors
    /// Rejects invalid apps, duplicate ids and already-tracked source identities.
    pub fn plan_import_apps(&self, apps: Vec<TrackedApp>) -> Result<Vec<TrackedApp>> {
        let apps: Vec<_> = apps
            .into_iter()
            .map(|app| {
                normalized(TrackedApp {
                    auto_update: false,
                    ..app
                })
            })
            .collect::<Result<_>>()?;
        if apps.is_empty() {
            return Ok(Vec::new());
        }
        validate_batch(&self.store.config()?.apps, &apps)?;
        Ok(apps)
    }

    /// Atomically adds validated selections. Imports always start with auto-update off.
    ///
    /// # Errors
    /// Rejects invalid apps, duplicate ids and already-tracked source identities.
    pub fn import_apps(&self, apps: Vec<TrackedApp>) -> Result<Vec<AppView>> {
        let apps = self.plan_import_apps(apps)?;
        if apps.is_empty() {
            return Ok(Vec::new());
        }
        let state = self.store.state()?;
        self.store.update_config(|mut config| {
            // Recheck the current config while holding the store lock: a plan is not a reservation.
            validate_batch(&config.apps, &apps)?;
            let mut views = Vec::with_capacity(apps.len());
            for app in apps {
                views.push(AppView::new(app.clone(), state.app(&app.id), false));
                config.apps.push(app);
            }
            Ok((config, views))
        })
    }

    /// Editable examples; choosing a recipe does not install or persist it.
    #[must_use]
    pub fn recipes(&self) -> Vec<AppRecipe> {
        crate::recipes::recipes()
    }
}

fn validate_batch(existing: &[TrackedApp], apps: &[TrackedApp]) -> Result<()> {
    for (index, app) in apps.iter().enumerate() {
        let mut prior = existing.iter().chain(&apps[..index]);
        if prior.clone().any(|other| other.id == app.id) {
            return Err(Error::AppExists(app.id.clone()));
        }
        if let Some(other) = prior.find(|other| other.source.same_identity(&app.source)) {
            return Err(Error::InvalidInput(format!(
                "source already tracked by `{}`",
                other.id
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
