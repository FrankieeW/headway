use crate::{
    AppView, Error, Headway, Install, RepositoryOption, RepositorySettings, Result, Source, Track,
};

impl Headway {
    /// The current repository plus saved alternatives; no network request or state change.
    pub fn repositories(&self, id: &str) -> Result<Vec<RepositoryOption>> {
        let config = self.store().config()?;
        let source = config.app(id)?.require_github("repository choices")?;
        let current = source.repo_id()?.full_name();
        let mut entries = source.options.repositories.clone();
        for entry in &mut entries {
            entry.repo = crate::repo::RepoId::parse(&entry.repo)?.full_name();
            if entry.label.is_empty() {
                entry.label.clone_from(&entry.repo);
            }
        }
        let index = entries
            .iter()
            .position(|entry| entry.repo.eq_ignore_ascii_case(&current));
        let snapshot = RepositoryOption {
            repo: current.clone(),
            label: index.map_or(current, |i| entries[i].label.clone()),
            git_url: source.git_url.clone(),
            branch: source
                .options
                .variant
                .as_ref()
                .and_then(|variant| variant.branch.clone())
                .or_else(|| match &source.track {
                    Track::Commit { branch } => branch.clone(),
                    _ => None,
                }),
            settings: Some(RepositorySettings {
                track: source.track.clone(),
                install: source.install.clone(),
                source_build: source.options.source_build.clone(),
                release_asset: source.options.release_asset.clone(),
            }),
        };
        if let Some(index) = index {
            entries[index] = snapshot;
        } else {
            entries.insert(0, snapshot);
        }
        Ok(entries)
    }

    /// Adds GitHub's parent/root origins without activating them or replacing saved recipes.
    pub async fn refresh_repositories(&self, id: &str) -> Result<AppView> {
        let original = self.store().config()?.app(id)?.clone();
        let info = self.github().await?.repo(&original.repo_id()?).await?;
        let found = crate::repository_family::choices(&info)?;
        let lock = crate::lock::AppLock::acquire_app_async(self.paths(), id).await?;
        self.store().update_config(|mut config| {
            let app = config
                .apps
                .iter_mut()
                .find(|app| app.id == id)
                .ok_or_else(|| Error::AppNotFound(id.into()))?;
            if app.source != original.source {
                return Err(Error::ConfigChanged(id.into()));
            }
            let Source::Github(github) = &mut app.source else {
                return Err(Error::InvalidInput(
                    "repository discovery requires GitHub".into(),
                ));
            };
            for entry in found {
                if !github
                    .options
                    .repositories
                    .iter()
                    .any(|saved| saved.repo.eq_ignore_ascii_case(&entry.repo))
                {
                    github.options.repositories.push(entry);
                }
            }
            app.validate()?;
            Ok((config, ()))
        })?;
        drop(lock);
        self.get(id)
    }

    /// Explicitly changes the update origin. The installed payload remains in place;
    /// `replace()` clears revision certainty and holds the normal app lock.
    pub fn select_repository(&self, id: &str, repository: &str) -> Result<AppView> {
        let requested = crate::repo::RepoId::parse(repository)?.full_name();
        let mut app = self.store().config()?.app(id)?.clone();
        let installed = self.store().state()?.app(id).installed;
        let entries = self.repositories(id)?;
        let choice = entries
            .iter()
            .find(|entry| {
                crate::repo::RepoId::parse(&entry.repo)
                    .is_ok_and(|repo| repo.full_name().eq_ignore_ascii_case(&requested))
            })
            .ok_or_else(|| {
                Error::InvalidInput("repository is not one of this app's configured origins".into())
            })?;
        let Source::Github(github) = &mut app.source else {
            return Err(Error::InvalidInput(
                "repository choices require a GitHub source".into(),
            ));
        };
        if github
            .repo_id()?
            .full_name()
            .eq_ignore_ascii_case(&requested)
        {
            return self.get(id);
        }
        github.repo = requested;
        github.git_url.clone_from(&choice.git_url);
        let destination = match &github.install {
            Install::Source(install) => install.dest.clone(),
            Install::Asset(install) => install.dest.clone(),
        }
        .or_else(|| installed.and_then(|value| value.path));
        if let Some(settings) = &choice.settings {
            github.track.clone_from(&settings.track);
            github.install.clone_from(&settings.install);
            github
                .options
                .source_build
                .clone_from(&settings.source_build);
            github
                .options
                .release_asset
                .clone_from(&settings.release_asset);
        }
        // Origins share one installation. A saved recipe must never move that payload.
        match &mut github.install {
            Install::Source(install) => install.dest = destination,
            Install::Asset(install) => install.dest = destination,
        }
        if let Some(variant) = &mut github.options.variant {
            variant.branch.clone_from(&choice.branch);
            github.track = Track::Commit {
                branch: Some(variant.reference.clone()),
            };
        } else if let Track::Commit { branch } = &mut github.track {
            branch.clone_from(&choice.branch);
        }
        github.options.repositories = entries;
        app.auto_update = false;
        self.replace(app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::cli_app;
    use crate::{InstalledRef, paths::Paths};

    #[test]
    fn official_and_fork_switches_preserve_payload_and_forget_foreign_revision() {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(Paths::under(dir.path()))
            .build()
            .unwrap();
        let mut app = cli_app("tool");
        let Source::Github(github) = &mut app.source else {
            unreachable!()
        };
        github.options.repositories.push(RepositoryOption {
            settings: None,
            repo: "fork/tool".into(),
            label: "My fork".into(),
            branch: Some("develop".into()),
            git_url: None,
        });
        hw.add(app).unwrap();
        let payload = dir.path().join("installed");
        std::fs::write(&payload, "unchanged").unwrap();
        hw.store()
            .update_state(|state| {
                Ok((
                    state.with_app("tool", |state| crate::state::AppState {
                        installed: Some(InstalledRef {
                            id: "a".repeat(40),
                            sha: Some("a".repeat(40)),
                            version: None,
                            path: Some(payload.display().to_string()),
                            installed_at: chrono::Utc::now(),
                        }),
                        ..state
                    }),
                    (),
                ))
            })
            .unwrap();
        let switched = hw.select_repository("tool", "FORK/tool").unwrap();
        assert!(switched.installed.is_none());
        assert_eq!(switched.app.repo_id().unwrap().owner, "FORK");
        assert_eq!(std::fs::read_to_string(&payload).unwrap(), "unchanged");
        assert_eq!(hw.repositories("tool").unwrap().len(), 2);
        let original = hw.select_repository("tool", "owner/tool").unwrap();
        assert_eq!(original.app.repo_id().unwrap().owner, "owner");
        assert!(hw.select_repository("tool", "unconfigured/tool").is_err());
    }

    #[test]
    fn changing_an_independent_origin_preserves_its_pin_and_changes_history_branch() {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(Paths::under(dir.path()))
            .build()
            .unwrap();
        let mut app = cli_app("separate");
        let original = "a".repeat(40);
        let Source::Github(github) = &mut app.source else {
            unreachable!()
        };
        github.track = Track::Commit {
            branch: Some(original.clone()),
        };
        github.options.variant = Some(crate::VariantInfo {
            parent_id: "parent".into(),
            reference: original.clone(),
            release_tag: None,
            branch: Some("main".into()),
        });
        github.options.repositories.push(RepositoryOption {
            settings: None,
            repo: "fork/tool".into(),
            label: "Fork".into(),
            git_url: None,
            branch: Some("work".into()),
        });
        hw.add(app).unwrap();
        let switched = hw.select_repository("separate", "fork/tool").unwrap();
        let github = switched.app.github().unwrap();
        assert_eq!(
            github.track,
            Track::Commit {
                branch: Some(original)
            }
        );
        assert_eq!(
            github.options.variant.as_ref().unwrap().branch.as_deref(),
            Some("work")
        );
        assert!(switched.installed.is_none());
    }

    #[test]
    fn alternatives_roundtrip_and_validate_canonical_duplicates() {
        let mut app = cli_app("tool");
        let Source::Github(github) = &mut app.source else {
            unreachable!()
        };
        github.options.repositories = vec![RepositoryOption {
            settings: None,
            repo: "fork/tool".into(),
            label: "Fork".into(),
            git_url: None,
            branch: None,
        }];
        let encoded = serde_json::to_value(&app).unwrap();
        assert!(encoded.get("options").is_none());
        assert_eq!(encoded["repositories"][0]["repo"], "fork/tool");
        let decoded: crate::TrackedApp = serde_json::from_value(encoded).unwrap();
        decoded.validate().unwrap();
        let Source::Github(github) = &mut app.source else {
            unreachable!()
        };
        github.options.repositories.push(RepositoryOption {
            settings: None,
            repo: "FORK/tool".into(),
            label: "duplicate".into(),
            git_url: None,
            branch: None,
        });
        assert!(app.validate().is_err());
    }
}
