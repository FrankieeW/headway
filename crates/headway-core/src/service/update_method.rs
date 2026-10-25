use crate::{InstalledRef, Result, RevisionMethod, Source, TrackedApp};

/// Selects a one-off recipe before resolving a target or running source probes.
pub(super) fn run_recipe(
    app: &TrackedApp,
    method: RevisionMethod,
    installed: Option<&InstalledRef>,
) -> Result<TrackedApp> {
    if method == RevisionMethod::Configured {
        return Ok(app.clone());
    }
    let github = app.require_github("one-off install methods")?;
    let install = method.recipe(
        github.source_recipe(),
        github.asset_recipe(),
        github.one_off_destination(installed),
    )?;
    Ok(TrackedApp {
        source: Source::Github(crate::GithubSource {
            install,
            ..github.clone()
        }),
        ..app.clone()
    })
}
