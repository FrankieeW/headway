use crate::{Error, Headway, Install, Result, RevisionOptions, TrackedApp, github::PullRequest};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PullRequestPage {
    pub repo: String,
    pub page: u32,
    pub per_page: u32,
    pub pull_requests: Vec<PullRequest>,
    pub has_more: bool,
}

impl Headway {
    /// Lists open pull requests; an exact number can also resolve a closed PR.
    pub async fn list_pull_requests(
        &self,
        id: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PullRequestPage> {
        crate::revision_history::validate_page(page, per_page)?;
        let app = self.store().config()?.app(id)?.clone();
        let repo = app.repo_id()?;
        let pull_requests = self
            .github()
            .await?
            .pull_requests(&repo, page, per_page)
            .await?;
        Ok(PullRequestPage {
            repo: repo.full_name(),
            page,
            per_page,
            has_more: pull_requests.len() == usize::try_from(per_page).unwrap_or(usize::MAX),
            pull_requests,
        })
    }

    pub(super) async fn pull_request_options(
        &self,
        app: &TrackedApp,
        reference: &str,
    ) -> Result<RevisionOptions> {
        let input = reference
            .strip_prefix("pr:")
            .ok_or_else(|| Error::InvalidInput("expected pr:<number>".into()))?;
        let (number, expected) = input
            .split_once('@')
            .map_or((input, None), |(number, sha)| (number, Some(sha)));
        let number = number
            .parse::<u64>()
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(|| Error::InvalidInput("PR number must be a positive integer".into()))?;
        if expected.is_some_and(|sha| !crate::revision_history::is_full_sha(sha)) {
            return Err(Error::InvalidInput(
                "PR pin must be a full commit SHA".into(),
            ));
        }
        let source = app.require_github("PR installation")?;
        let repo = source.repo_id()?;
        let gh = self.github().await?;
        let pull = gh.pull_request(&repo, number).await?;
        if pull.number != number
            || !pull
                .base
                .repo
                .as_ref()
                .is_some_and(|base| base.full_name.eq_ignore_ascii_case(&repo.full_name()))
            || !crate::revision_history::is_full_sha(&pull.head.sha)
        {
            return Err(Error::InvalidInput(
                "GitHub returned an invalid PR identity".into(),
            ));
        }
        if expected.is_some_and(|sha| !sha.eq_ignore_ascii_case(&pull.head.sha)) {
            return Err(Error::InvalidInput(
                "PR head changed since review; inspect the PR again and confirm its new commit"
                    .into(),
            ));
        }
        let commit = gh.commit(&repo, &pull.head.sha).await?;
        if !commit.sha.eq_ignore_ascii_case(&pull.head.sha) {
            return Err(Error::InvalidInput(
                "PR head did not resolve to the selected commit".into(),
            ));
        }
        let source_build = match &source.install {
            Install::Source(build) => Some(build.clone()),
            Install::Asset(_) => source.options.source_build.clone(),
        };
        Ok(RevisionOptions {
            reference: format!("pr:{number}@{}", pull.head.sha),
            commit: pull.head.sha.clone(),
            candidate: crate::check::commit_candidate(&commit),
            release_tag: None,
            release_url: None,
            can_download: false,
            source_build,
            release_asset: None,
            pull_request: Some(pull),
        })
    }
}
