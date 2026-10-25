use super::update_method::run_recipe;
use super::*;

use crate::revision_install::RevisionMethod;
use crate::update::UpdateRequest;

impl Headway {
    /// What `update` would do, without doing it.
    pub async fn plan(&self, id: &str, target: &Target) -> Result<UpdatePlan> {
        self.plan_with_cancel(id, target, &CancellationToken::new())
            .await
    }

    /// What `update` would do with a forced install method, without doing it.
    pub async fn plan_with(&self, id: &str, request: &UpdateRequest) -> Result<UpdatePlan> {
        self.plan_request(id, request, &CancellationToken::new())
            .await
    }

    pub async fn plan_with_cancel(
        &self,
        id: &str,
        target: &Target,
        cancel: &CancellationToken,
    ) -> Result<UpdatePlan> {
        self.plan_request(
            id,
            &UpdateRequest {
                target: target.clone(),
                method: RevisionMethod::Configured,
            },
            cancel,
        )
        .await
    }

    async fn plan_request(
        &self,
        id: &str,
        request: &UpdateRequest,
        cancel: &CancellationToken,
    ) -> Result<UpdatePlan> {
        let target = &request.target;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let config = self.store.config()?;
        let app = config.app(id)?.clone();
        let pending = crate::recovery::pending(self.paths(), &app);
        let mut installed = if pending {
            None
        } else {
            super::presence::retained(&app, self.store.state()?.app(id).installed)?
        };
        let recipe = run_recipe(&app, request.method, installed.as_ref())?;
        let gh = self.github_for(&app).await?;
        if let (Source::Npm(source), Target::Ref(_)) = (&app.source, target) {
            let version =
                crate::npm::installed_version(cancel, &self.env().await, &source.package).await?;
            installed = super::checks::sync_installed(
                AppState {
                    installed,
                    ..AppState::default()
                },
                &InstalledSync::Detected(version),
            )
            .installed;
        }
        let (candidate, checked) = self
            .resolve(&gh, &app, installed.as_ref(), target, cancel)
            .await?;
        if let Some(checked) = &checked {
            installed = super::checks::sync_installed(
                AppState {
                    installed,
                    ..AppState::default()
                },
                &checked.installed,
            )
            .installed;
        }
        let already = installed.as_ref().is_some_and(|installed| {
            candidate.id != crate::script::LATEST
                && target_matches(&app, &installed.id, &candidate.id)
        }) || (matches!(app.source, Source::Npm(_))
            && checked
                .as_ref()
                .is_some_and(|checked| checked.result.status == UpdateStatus::UpToDate));
        let explicit = matches!(target, Target::Ref(_));
        // A forced method decides how the run installs, not what it installs.
        let asset = match recipe.github().map(|github| &github.install) {
            Some(crate::config::Install::Asset(spec)) => {
                let release = update::release_for_target(&gh, &recipe, &candidate).await?;
                crate::asset::select(
                    &release.assets,
                    app.kind,
                    spec.pattern.as_deref(),
                    crate::asset::Arch::host(),
                )
                .map(|asset| Some(asset.clone()))?
            }
            Some(crate::config::Install::Source(_)) | None => None,
        };
        let mut steps = update::plan_steps(
            &recipe,
            &config.settings,
            self.paths(),
            &candidate,
            asset.as_ref(),
        );
        if pending {
            steps.insert(0, "An installation is pending recovery; recorded baseline is not trusted. Run a check after the active update finishes.".into());
        }
        Ok(UpdatePlan {
            app_id: app.id.clone(),
            // A real run skips an installed target, except an explicit `--to`, which reinstalls.
            up_to_date: already && !explicit,
            reinstall: already && explicit,
            from: installed,
            to: candidate,
            method: recipe.source.method(),
            steps,
            asset: asset.map(|asset| update::AssetChoice {
                name: asset.name,
                size: asset.size,
                digest: asset.digest,
            }),
        })
    }

    /// Turns a [`Target`] into a concrete candidate. `Latest`/`Next` run a check,
    /// which is returned so callers can cache it.
    pub(super) async fn resolve(
        &self,
        gh: &GitHub,
        app: &TrackedApp,
        installed: Option<&InstalledRef>,
        target: &Target,
        cancel: &CancellationToken,
    ) -> Result<(Candidate, Option<Checked>)> {
        if !app.source.picks_versions() && !matches!(target, Target::Latest) {
            return Err(Error::InvalidInput(format!(
                "{} sources only support the latest target",
                app.source.kind()
            )));
        }
        let env = self.env().await;
        let reference = match target {
            Target::Latest | Target::Next => {
                let checked = super::presence::check(
                    &Probe {
                        gh,
                        env: &env,
                        cancel,
                    },
                    app,
                    installed,
                )
                .await?;
                let result = &checked.result;
                let stepping = matches!(target, Target::Next)
                    && matches!(
                        result.status,
                        UpdateStatus::UpdateAvailable | UpdateStatus::Diverged
                    );
                let candidate = if stepping {
                    result.next()
                } else {
                    result.latest.as_ref()
                };
                let candidate = match candidate.cloned() {
                    Some(candidate) => candidate,
                    None if matches!(app.source, Source::Script(_)) => {
                        crate::script::unknown_target()
                    }
                    None => {
                        return Err(Error::RemoteNotFound(format!(
                            "no version found for {}",
                            app.source.label()
                        )));
                    }
                };
                return Ok((candidate, Some(checked)));
            }
            Target::Ref(reference) => reference,
        };
        if let Source::Npm(source) = &app.source {
            return Ok((
                crate::npm::version(cancel, &env, source, reference).await?,
                None,
            ));
        }
        let github = app.require_github("version selection")?;
        let repo = app.repo_id()?;
        let candidate = match &github.track {
            Track::Commit { .. } => check::commit_candidate(&gh.commit(&repo, reference).await?),
            Track::Release { .. } => {
                check::release_candidate(&gh.release_by_tag(&repo, reference).await?)
            }
            Track::Tag { .. } => {
                let commit = check::commit_candidate(&gh.commit(&repo, reference).await?);
                Candidate {
                    id: reference.clone(),
                    title: reference.clone(),
                    body: None,
                    url: Some(format!("{}/tree/{reference}", repo.html_url())),
                    prerelease: Version::parse(reference)
                        .is_some_and(|version| version.is_prerelease()),
                    ..commit
                }
            }
        };
        Ok((candidate, None))
    }
}
