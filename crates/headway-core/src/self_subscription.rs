//! Adopt the installed desktop app as the first subscription without guessing a baseline.

use crate::{
    AppKind, AppView, AutoUpdateMode, Error, GithubSource, Headway, Install, InstalledRef, Result,
    Source, Track, TrackedApp,
};

fn is_self(app: &TrackedApp) -> bool {
    if app.kind != AppKind::App {
        return false;
    }
    match &app.source {
        Source::Github(github) => github.repo_id().is_ok_and(|repo| {
            repo.owner.eq_ignore_ascii_case("FrankieeW")
                && repo.name.eq_ignore_ascii_case("headway")
        }),
        Source::Brew(brew) => brew.cask && brew.package.eq_ignore_ascii_case("headway"),
        Source::Npm(_) | Source::Script(_) => false,
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "self_subscription_tests.rs"]
mod tests;

fn matches_destination(
    github: &GithubSource,
    apps_dir: &str,
    installed: Option<InstalledRef>,
    bundle: &std::path::Path,
) -> bool {
    let (configured, artifact) = match &github.install {
        Install::Source(source) => (source.dest.as_deref(), source.artifact.as_deref()),
        Install::Asset(asset) => (asset.dest.as_deref(), asset.binary.as_deref()),
    };
    let root = crate::paths::expand_tilde(apps_dir);
    let recorded = installed
        .and_then(|installed| installed.path)
        .map(|path| crate::paths::expand_tilde(&path))
        .filter(|path| {
            path.parent().and_then(|parent| parent.canonicalize().ok()) == root.canonicalize().ok()
        });
    configured
        .map(crate::paths::expand_tilde)
        .or_else(|| {
            artifact
                .and_then(|artifact| std::path::Path::new(artifact).file_name())
                .filter(|name| !name.to_string_lossy().contains(['*', '?', '[']))
                .map(|name| root.join(name))
        })
        .or(recorded)
        .and_then(|path| path.canonicalize().ok())
        .zip(bundle.canonicalize().ok())
        .is_some_and(|(expected, observed)| expected == observed)
}

fn new_self_app(config: &crate::Config, dest: &str) -> Result<TrackedApp> {
    let mut app = crate::recipes::recipes()
        .into_iter()
        .find(|recipe| recipe.id == "headway-app")
        .ok_or_else(|| Error::Config("Headway app recipe is unavailable".into()))?
        .app;
    app.id = "headway".into();
    let mut suffix = 1_u32;
    while config.apps.iter().any(|existing| existing.id == app.id) {
        app.id = format!("headway-self-{suffix}");
        suffix = suffix
            .checked_add(1)
            .ok_or_else(|| Error::Config("no free self subscription id".into()))?;
    }
    app.auto_update = false;
    app.update_policy.mode = AutoUpdateMode::Notify;
    let Source::Github(github) = &mut app.source else {
        return Err(Error::Config("Headway recipe must use GitHub".into()));
    };
    let Install::Source(source) = &mut github.install else {
        return Err(Error::Config(
            "Headway recipe must build from source".into(),
        ));
    };
    source.dest = Some(dest.into());
    source.verify_command =
        Some("\"$HEADWAY_INSTALLED_PATH/Contents/MacOS/headway\" --json version".into());
    Ok(app)
}

#[path = "self_subscription/provenance.rs"]
mod provenance;

impl Headway {
    /// Ensures that the installed Headway desktop app is the first tracked subscription.
    /// Existing self configuration is preserved; new subscriptions only notify.
    pub async fn ensure_self_subscription(&self) -> Result<AppView> {
        if let Some(id) = self.running_variant_id()? {
            return self.get(&id);
        }
        let bundle = crate::cli_link::bundle(self, None).await?.ok_or_else(|| {
            Error::InvalidInput("no installed Headway desktop app was found".into())
        })?;
        // Probe the installed build before touching config: a rejected/garbled probe must not
        // leave a half-registered subscription behind, and the result is reused below.
        let status = self.self_update_status().await?;
        let dest = bundle.display().to_string();
        let observed = self.store().state()?;
        let (id, created) = self.store().update_config(|mut config| {
            let found = config.apps.iter().position(|app| {
                is_self(app)
                    || (app.kind == AppKind::App
                        && app.github().is_some_and(|github| {
                            matches_destination(
                                github,
                                &config.settings.apps_dir,
                                observed.app(&app.id).installed,
                                &bundle,
                            )
                        }))
            });
            let created = found.is_none();
            let app = match found {
                Some(index) => config.apps.remove(index),
                None => new_self_app(&config, &dest)?,
            };
            let id = app.id.clone();
            config.apps.insert(0, app);
            Ok((config, (id, created)))
        })?;
        // Use the same lock as update/restore, then observe the exact managed target.
        let lock = crate::lock::AppLock::acquire_app(self.paths(), &id)?;
        let config = self.store().config()?;
        let app = config.app(&id)?;
        let Some(github) = app
            .github()
            .filter(|github| matches!(github.track, Track::Commit { .. }))
        else {
            drop(lock);
            return self.get(&id);
        };
        if !matches_destination(
            github,
            &config.settings.apps_dir,
            self.store().state()?.app(&id).installed,
            &bundle,
        ) {
            drop(lock);
            return self.get(&id);
        }
        if let Some(installed) = status.installed {
            self.sync_self_baseline(&id, created, &dest, &installed)?;
        }
        drop(lock);
        self.get(&id)
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "self_subscription_metadata_tests.rs"]
mod metadata_missing_tests;
