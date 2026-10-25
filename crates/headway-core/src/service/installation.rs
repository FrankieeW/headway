use super::presence::{self, Presence};
use crate::{
    Headway, Result, TrackedApp,
    state::{AppState, RetainedInstallation},
};
use serde::Serialize;

/// Filesystem evidence, separate from the selected repository's revision baseline.
#[derive(Debug, Clone, Serialize)]
pub struct InstallationDetection {
    pub presence: Presence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub baseline_matches: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded: Option<RetainedInstallation>,
}

pub(super) fn detect(app: &TrackedApp, state: &AppState) -> Result<InstallationDetection> {
    let retained = state.retained_installation.as_ref();
    let installed = state
        .installed
        .as_ref()
        .or_else(|| retained.map(|value| &value.installed));
    let presence = presence::observe(app, installed)?;
    let path = presence::location(app, installed)
        .map(|path| crate::paths::display_path(&crate::paths::expand_tilde(path)));
    let baseline_matches = !matches!(presence, Presence::Missing | Presence::Invalid)
        && presence::has_baseline(app, state.installed.as_ref());
    let recorded = if matches!(presence, Presence::Present | Presence::Unknown) {
        state
            .installed
            .as_ref()
            .and_then(|installed| {
                app.github().map(|github| RetainedInstallation {
                    repo: github.repo.clone(),
                    installed: installed.clone(),
                })
            })
            .or_else(|| retained.cloned())
    } else {
        None
    };
    Ok(InstallationDetection {
        presence,
        path,
        baseline_matches,
        recorded,
    })
}

impl Headway {
    /// Reads local payload evidence without executing the installed program or changing state.
    pub fn detect_installation(&self, id: &str) -> Result<InstallationDetection> {
        detect(
            self.store().config()?.app(id)?,
            &self.store().state()?.app(id),
        )
    }
}
