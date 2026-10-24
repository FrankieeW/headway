use super::Job;
use crate::github::Release;
use crate::state::Candidate;
use crate::{Error, Result};

impl Job<'_> {
    /// An asset must belong to an actual release at the requested commit.
    pub(super) async fn release_for_target(&self, target: &Candidate) -> Result<Release> {
        let repo = self.app.repo_id()?;
        match self.gh.release_by_tag(&repo, &target.id).await {
            Ok(release) => return self.verify_release_commit(release, target).await,
            Err(Error::RemoteNotFound(_)) => {}
            Err(error) => return Err(error),
        }
        let sha = target.sha.as_deref().unwrap_or(&target.id);
        if let Some(variant) = &self.app.require_github("release assets")?.options.variant
            && variant.reference.eq_ignore_ascii_case(sha)
            && let Some(tag) = &variant.release_tag
        {
            return self
                .verify_release_commit(self.gh.release_by_tag(&repo, tag).await?, target)
                .await;
        }
        let tags = self.gh.tags(&repo).await?;
        for tag in tags
            .into_iter()
            .filter(|tag| tag.commit.sha.eq_ignore_ascii_case(sha))
        {
            match self.gh.release_by_tag(&repo, &tag.name).await {
                Ok(release) => return self.verify_release_commit(release, target).await,
                Err(Error::RemoteNotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Err(Error::RemoteNotFound(format!(
            "no published release found for commit {sha}; choose a source build or an exact release tag"
        )))
    }

    async fn verify_release_commit(&self, release: Release, target: &Candidate) -> Result<Release> {
        if release.draft {
            return Err(Error::InvalidInput(
                "draft releases cannot be installed".into(),
            ));
        }
        if let Some(sha) = &target.sha {
            let actual = self
                .gh
                .commit(
                    &self.app.repo_id()?,
                    &format!("refs/tags/{}", release.tag_name),
                )
                .await?;
            if !actual.sha.eq_ignore_ascii_case(sha) {
                return Err(Error::InvalidInput(
                    "release tag does not identify the selected commit".into(),
                ));
            }
        }
        Ok(release)
    }
}
