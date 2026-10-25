use super::identity::SourceIdentity;
use crate::config::{Install, TrackedApp};
use crate::error::{Error, Result};
use crate::install;
use crate::paths::{Paths, expand_tilde};
use crate::state::InstalledRef;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub(super) struct Intent {
    app_id: String,
    identity: SourceIdentity,
    dest: PathBuf,
    candidate: Option<InstalledRef>,
    previous: Option<InstalledRef>,
    verified: bool,
    #[serde(default)]
    previous_history: Option<chrono::DateTime<chrono::Utc>>,
}

fn file(paths: &Paths, app: &TrackedApp) -> Result<PathBuf> {
    super::component(&app.id)?;
    let root = paths.config_dir.join("transactions");
    if paths.config_dir.is_symlink() || root.is_symlink() {
        return Err(Error::InvalidInput(
            "transaction directories cannot be symlinks".into(),
        ));
    }
    Ok(root.join(format!("{}.json", app.id)))
}

pub(crate) fn pending(paths: &Paths, app: &TrackedApp) -> bool {
    file(paths, app).is_ok_and(|file| file.exists() || file.is_symlink())
}

impl Intent {
    pub(super) fn create(
        paths: &Paths,
        app: &TrackedApp,
        dest: &Path,
        candidate: Option<&InstalledRef>,
        previous: Option<InstalledRef>,
    ) -> Result<Self> {
        let intent = Self {
            app_id: app.id.clone(),
            identity: SourceIdentity::from_app(app)?,
            dest: dest.to_owned(),
            candidate: candidate.cloned(),
            previous,
            verified: false,
            previous_history: Store::new(paths.clone())
                .state()?
                .app(&app.id)
                .history
                .last()
                .map(|entry| entry.at),
        };
        intent.validate(paths, app)?;
        intent.save(paths, app)?;
        Ok(intent)
    }
    pub(super) fn verified(&mut self, paths: &Paths, app: &TrackedApp) -> Result<()> {
        self.verified = true;
        self.save(paths, app)
    }
    fn save(&self, paths: &Paths, app: &TrackedApp) -> Result<()> {
        let file = file(paths, app)?;
        let parent = file
            .parent()
            .ok_or_else(|| Error::InvalidInput("journal has no parent".into()))?;
        fs::create_dir_all(parent).map_err(|error| Error::io_at("create", parent, error))?;
        install::sync_dir(&paths.config_dir)?;
        let bytes =
            serde_json::to_vec(self).map_err(|error| Error::parse("recovery journal", &error))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| Error::io("create recovery journal", error))?;
        temp.write_all(&bytes)
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|error| Error::io("sync recovery journal", error))?;
        temp.persist(&file)
            .map_err(|error| Error::io_at("save", &file, error.error))?;
        install::sync_dir(parent)
    }
    fn validate(&self, paths: &Paths, app: &TrackedApp) -> Result<()> {
        if self.app_id != app.id
            || self.identity != SourceIdentity::from_app(app)?
            || !self.dest.is_absolute()
            || self
                .dest
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(Error::InvalidInput(
                "recovery journal identity/destination mismatch".into(),
            ));
        }
        let store = Store::new(paths.clone());
        let config = store.config()?;
        let configured = match &app.require_github("recovery")?.install {
            Install::Source(source) => source.dest.as_deref(),
            Install::Asset(asset) => asset.dest.as_deref(),
        };
        let name = self
            .dest
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::InvalidInput("invalid journal destination".into()))?;
        if !crate::asset::safe_file_name(name) {
            return Err(Error::InvalidInput(
                "unsafe journal destination name".into(),
            ));
        }
        let expected = install::resolve_dest(configured, app.kind, name, &config.settings);
        if !same_destination(&expected, &self.dest) {
            return Err(Error::InvalidInput(
                "journal destination no longer matches installation settings".into(),
            ));
        }
        crate::config::validate_destination(&config, &app.id, &self.dest)?;
        for (id, state) in &store.state()?.apps {
            let paths = state
                .installed
                .as_ref()
                .and_then(|installed| installed.path.as_deref())
                .into_iter()
                .chain(
                    state
                        .retained_installation
                        .as_ref()
                        .and_then(|retained| retained.installed.path.as_deref()),
                );
            if id != &app.id
                && paths
                    .into_iter()
                    .any(|path| same_destination(&expand_tilde(path), &self.dest))
            {
                return Err(Error::InvalidInput(
                    "installation destination belongs to another app".into(),
                ));
            }
        }
        for other in config.apps.iter().filter(|other| other.id != app.id) {
            if crate::config::destination(other, &config.settings)
                .is_some_and(|path| same_destination(&path, &self.dest))
            {
                return Err(Error::InvalidInput(
                    "installation destination aliases another app's path".into(),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn clear(paths: &Paths, app: &TrackedApp) -> Result<()> {
    let file = file(paths, app)?;
    super::cache::cleanup_pending(paths, &app.id)?;
    match fs::remove_file(&file) {
        Ok(()) => install::sync_dir(
            file.parent()
                .ok_or_else(|| Error::InvalidInput("missing journal parent".into()))?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io_at("clear", &file, error)),
    }
}

fn load(paths: &Paths, app: &TrackedApp) -> Result<Option<Intent>> {
    let file = file(paths, app)?;
    if !file.exists() && !file.is_symlink() {
        return Ok(None);
    }
    if file.is_symlink()
        || fs::metadata(&file)
            .map_err(|error| Error::io_at("read", &file, error))?
            .len()
            > 64 * 1024
    {
        return Err(Error::InvalidInput("unsafe recovery journal".into()));
    }
    let intent: Intent = serde_json::from_slice(
        &fs::read(&file).map_err(|error| Error::io_at("read", &file, error))?,
    )
    .map_err(|error| Error::parse("recovery journal", &error))?;
    intent.validate(paths, app)?;
    Ok(Some(intent))
}

fn same_destination(left: &Path, right: &Path) -> bool {
    if crate::config::same_destination(left, right) {
        return true;
    }
    let resolved = |path: &Path| {
        path.parent()
            .and_then(|parent| parent.canonicalize().ok())
            .zip(path.file_name())
            .map(|(parent, name)| parent.join(name))
    };
    resolved(left)
        .zip(resolved(right))
        .is_some_and(|(left, right)| left == right)
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

/// Called under the app lock, before trusting the recorded installed baseline.
pub(crate) fn reconcile(paths: &Paths, app: &TrackedApp) -> Result<()> {
    let Some(intent) = load(paths, app)? else {
        return Ok(());
    };
    let store = Store::new(paths.clone());
    let state = store.state()?.app(&app.id);
    let current = state.installed;
    let state_committed = intent.verified
        && match (&intent.candidate, &current) {
            (Some(candidate), Some(current)) => {
                candidate.id == current.id
                    && candidate.sha == current.sha
                    && current
                        .path
                        .as_deref()
                        .is_some_and(|path| same_destination(&expand_tilde(path), &intent.dest))
                    && intent
                        .previous
                        .as_ref()
                        .is_none_or(|previous| previous.installed_at != current.installed_at)
            }
            (None, None) => state
                .history
                .last()
                .is_some_and(|entry| entry.ok && Some(entry.at) != intent.previous_history),
            _ => false,
        };
    install::reconcile(&intent.dest, state_committed)?;
    if !state_committed {
        store.update_state(|state| {
            Ok((
                state.with_app(&app.id, |state| crate::state::AppState {
                    installed: intent.previous.clone(),
                    last_check: None,
                    ..state
                }),
                (),
            ))
        })?;
    }
    clear(paths, app)
}

/// State/history is durable; now the old same-volume payload may be removed.
pub(crate) fn commit(paths: &Paths, app: &TrackedApp) -> Result<()> {
    let Some(intent) = load(paths, app)? else {
        return Ok(());
    };
    if !intent.verified {
        return Err(Error::InvalidInput(
            "cannot commit an unverified installation".into(),
        ));
    }
    install::reconcile(&intent.dest, true)?;
    clear(paths, app)
}
