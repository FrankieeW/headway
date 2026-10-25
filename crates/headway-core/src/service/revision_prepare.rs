use crate::{
    Error, Headway, Install, Result, RevisionMethod, RevisionOptions, RevisionRequest, Track,
    TrackedApp,
};

pub(super) struct PreparedRevision {
    pub app: TrackedApp,
    pub options: RevisionOptions,
    pub job_target: crate::Candidate,
    pub installed_id: String,
    pub asset: Option<crate::github::Asset>,
}

impl Headway {
    pub async fn revision_options(&self, id: &str, reference: &str) -> Result<RevisionOptions> {
        let app = self.store().config()?.app(id)?.clone();
        self.revision_options_for(&app, reference)
            .await
            .map(|(options, _)| options)
    }

    async fn revision_options_for(
        &self,
        app: &TrackedApp,
        reference: &str,
    ) -> Result<(RevisionOptions, Option<String>)> {
        if reference.starts_with("pr:") {
            return self
                .pull_request_options(app, reference)
                .await
                .map(|options| (options, None));
        }
        if reference.trim().is_empty()
            || reference.len() > 256
            || reference.contains(char::is_control)
        {
            return Err(Error::InvalidInput(
                "revision must name a commit, tag or branch".into(),
            ));
        }
        let github = app.require_github("commit installation")?;
        let gh = self.github_for(app).await?;
        let repo = github.repo_id()?;
        let commit = gh.commit(&repo, reference).await?;
        if commit.sha.len() != 40 || !commit.sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::InvalidInput(
                "GitHub returned an invalid commit".into(),
            ));
        }
        let metadata = self.commit_metadata(&app.id, &commit.sha, false).await?;
        let known_tag = metadata.tags.first().cloned();
        let mut release = None;
        // A user-supplied tag also works outside the bounded commit-badge tag window.
        let mut tags = metadata.tags;
        if let Some(variant) = &github.options.variant
            && variant.reference.eq_ignore_ascii_case(&commit.sha)
            && let Some(tag) = &variant.release_tag
            && !tags.contains(tag)
        {
            tags.insert(0, tag.clone());
        }
        if !reference.bytes().all(|b| b.is_ascii_hexdigit()) {
            tags.retain(|tag| tag != reference);
            tags.insert(0, reference.to_owned());
        }
        for tag in tags {
            match gh.release_by_tag(&repo, &tag).await {
                Ok(candidate) if !candidate.draft => {
                    let tagged = gh
                        .commit(&repo, &format!("refs/tags/{}", candidate.tag_name))
                        .await?;
                    if tagged.sha.eq_ignore_ascii_case(&commit.sha) {
                        release = Some(candidate);
                        break;
                    }
                }
                Ok(_) | Err(Error::RemoteNotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let source_build = github.source_recipe();
        let release_asset = github.asset_recipe();
        let can_download = release.as_ref().is_some_and(|release| {
            crate::asset::select(
                &release.assets,
                app.kind,
                release_asset.pattern.as_deref(),
                crate::asset::Arch::host(),
            )
            .is_ok()
        });
        Ok((
            RevisionOptions {
                pull_request: None,
                reference: reference.into(),
                commit: commit.sha.clone(),
                candidate: crate::check::commit_candidate(&commit),
                release_tag: release.as_ref().map(|r| r.tag_name.clone()),
                release_url: release.as_ref().and_then(|r| r.html_url.clone()),
                can_download,
                source_build,
                release_asset: Some(release_asset),
            },
            known_tag,
        ))
    }

    pub(super) async fn prepare_revision(
        &self,
        mut app: TrackedApp,
        request: &RevisionRequest,
    ) -> Result<PreparedRevision> {
        let (mut options, known_tag) = self.revision_options_for(&app, &request.reference).await?;
        let installed = self.store().state()?.app(&app.id).installed;
        let github = app.require_github("commit installation")?;
        let destination = github.one_off_destination(installed.as_ref());
        let method = match request.method {
            RevisionMethod::Configured if options.pull_request.is_some() => RevisionMethod::Source,
            RevisionMethod::Configured => match github.install {
                Install::Source(_) => RevisionMethod::Source,
                Install::Asset(_) => RevisionMethod::Asset,
            },
            method => method,
        };
        if options.pull_request.is_some()
            && (method == RevisionMethod::Asset || request.release_tag.is_some())
        {
            return Err(Error::InvalidInput(
                "PR versions require a source build; release assets do not identify a PR head"
                    .into(),
            ));
        }
        if let Some(tag) = &request.release_tag {
            if tag.is_empty() || tag.len() > 256 || tag.contains(char::is_control) {
                return Err(Error::InvalidInput("invalid release tag".into()));
            }
            let gh = self.github_for(&app).await?;
            let release = gh.release_by_tag(&app.repo_id()?, tag).await?;
            let actual = gh
                .commit(&app.repo_id()?, &format!("refs/tags/{}", release.tag_name))
                .await?;
            if release.draft || !actual.sha.eq_ignore_ascii_case(&options.commit) {
                return Err(Error::InvalidInput(
                    "release tag does not identify the selected commit".into(),
                ));
            }
            options.release_tag = Some(release.tag_name);
            options.release_url = release.html_url;
        }
        let job_target = if method == RevisionMethod::Asset {
            crate::Candidate {
                id: options.release_tag.clone().ok_or_else(|| {
                    Error::InvalidInput(
                        "this commit has no verified published release; choose a source build"
                            .into(),
                    )
                })?,
                ..options.candidate.clone()
            }
        } else {
            options.candidate.clone()
        };
        let install = method.recipe(
            request
                .source_build
                .clone()
                .or_else(|| options.source_build.clone()),
            request
                .release_asset
                .clone()
                .or_else(|| options.release_asset.clone())
                .unwrap_or_default(),
            destination,
        )?;
        let installed_id = if github.options.variant.is_some() {
            options.commit.clone()
        } else {
            match github.track {
                Track::Commit { .. } => options.commit.clone(),
                Track::Tag { .. } => options
                    .release_tag
                    .clone()
                    .or(known_tag)
                    .unwrap_or_else(|| options.commit.clone()),
                Track::Release { .. } => options
                    .release_tag
                    .clone()
                    .unwrap_or_else(|| options.commit.clone()),
            }
        };
        let asset = if let Install::Asset(spec) = &install {
            let gh = self.github_for(&app).await?;
            let release = gh.release_by_tag(&app.repo_id()?, &job_target.id).await?;
            Some(
                crate::asset::select(
                    &release.assets,
                    app.kind,
                    spec.pattern.as_deref(),
                    crate::asset::Arch::host(),
                )?
                .clone(),
            )
        } else {
            None
        };
        let crate::Source::Github(github) = &mut app.source else {
            return Err(Error::InvalidInput(
                "commit installation requires GitHub".into(),
            ));
        };
        github.install = install;
        Ok(PreparedRevision {
            app,
            options,
            job_target,
            installed_id,
            asset,
        })
    }
}
