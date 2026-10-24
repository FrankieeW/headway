//! Only a source-stamped bundle can supply a new repository baseline.
use crate::{BuildInfo, Error, Headway, InstalledRef, Result, Source, Track};

impl Headway {
    pub(super) fn sync_self_baseline(
        &self,
        id: &str,
        created: bool,
        dest: &str,
        info: &BuildInfo,
    ) -> Result<()> {
        let origin = info.repository.as_deref().and_then(|value| {
            crate::repo::RepoId::parse(value)
                .ok()
                .filter(|repo| repo.full_name() == value)
        });
        let commit = info
            .commit
            .as_ref()
            .filter(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()));
        if created && let Some(origin) = &origin {
            self.store().update_config(|mut config| {
                let app = config
                    .apps
                    .iter_mut()
                    .find(|app| app.id == id)
                    .ok_or_else(|| Error::AppNotFound(id.into()))?;
                if let Source::Github(github) = &mut app.source
                    && !github
                        .repo_id()?
                        .full_name()
                        .eq_ignore_ascii_case(&origin.full_name())
                {
                    github.repo = origin.full_name();
                    github.git_url = None;
                    github.track = Track::Commit { branch: None };
                }
                Ok((config, ()))
            })?;
        }
        let config = self.store().config()?;
        let expected = config.app(id)?.repo_id()?.full_name();
        let trusted = origin
            .as_ref()
            .is_some_and(|repo| repo.full_name().eq_ignore_ascii_case(&expected))
            && !info.dirty
            && commit.is_some();
        self.store().update_state(|state| {
            Ok((
                state.with_app(id, |state| {
                    if !trusted {
                        // Legacy builds can keep an installer-recorded matching baseline;
                        // missing origin never creates a new one after a source switch.
                        let known_legacy = origin.is_none()
                            && !info.dirty
                            && commit.is_some_and(|sha| {
                                state
                                    .installed
                                    .as_ref()
                                    .and_then(|i| i.sha.as_ref())
                                    .is_some_and(|known| known.eq_ignore_ascii_case(sha))
                            });
                        return if known_legacy {
                            state
                        } else {
                            crate::state::AppState {
                                installed: None,
                                last_check: None,
                                ..state
                            }
                        };
                    }
                    let Some(commit) = commit else {
                        return state;
                    };
                    if state.installed.as_ref().is_some_and(|current| {
                        current.id == *commit
                            && current.path.as_deref() == Some(dest)
                            && current.version.as_deref() == Some(info.version.as_str())
                    }) {
                        return state;
                    }
                    crate::state::AppState {
                        installed: Some(InstalledRef {
                            id: commit.clone(),
                            sha: Some(commit.clone()),
                            version: Some(info.version.clone()),
                            installed_at: chrono::Utc::now(),
                            path: Some(dest.into()),
                        }),
                        last_check: None,
                        last_error: None,
                        ..state
                    }
                }),
                (),
            ))
        })
    }
}
