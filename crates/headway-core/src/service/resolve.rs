use super::*;

impl Headway {
    /// What `update` would do, without doing it.
    pub async fn plan(&self, id: &str, target: &Target) -> Result<UpdatePlan> {
        let config = self.store.config()?;
        let app = config.app(id)?.clone();
        let cancel = CancellationToken::new();
        if app.source.detects_installed() {
            let _lock = AppLock::acquire(&self.paths().app_lock(id), id)?;
            self.refresh_for_update(&app, target, &cancel).await?;
        }
        let gh = self.github_for(&app).await?;
        let installed = self.store.state()?.app(id).installed;
        let (candidate, checked) = self
            .resolve(&gh, &app, installed.as_ref(), target, &cancel)
            .await?;
        let already = installed.as_ref().is_some_and(|installed| {
            candidate.id != crate::script::LATEST && ids_match(&installed.id, &candidate.id)
        }) || (matches!(app.source, Source::Npm(_))
            && checked
                .as_ref()
                .is_some_and(|checked| checked.result.status == UpdateStatus::UpToDate));
        let explicit = matches!(target, Target::Ref(_));
        let asset = match app.github().map(|github| &github.install) {
            Some(crate::config::Install::Asset(spec)) => {
                let release = gh.release_by_tag(&app.repo_id()?, &candidate.id).await?;
                crate::asset::select(
                    &release.assets,
                    app.kind,
                    spec.pattern.as_deref(),
                    crate::asset::Arch::host(),
                )
                .ok()
                .cloned()
            }
            Some(crate::config::Install::Source(_)) | None => None,
        };
        let steps = update::plan_steps(
            &app,
            &config.settings,
            self.paths(),
            &candidate,
            asset.as_ref(),
        );
        Ok(UpdatePlan {
            app_id: app.id.clone(),
            // A real run skips an installed target, except an explicit `--to`, which reinstalls.
            up_to_date: already && !explicit,
            reinstall: already && explicit,
            from: installed,
            to: candidate,
            method: app.source.method(),
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
                let checked = check::check(
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
