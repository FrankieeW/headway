use super::Job;
use crate::config::TrackedApp;
use crate::github::{GitHub, Release};
use crate::state::Candidate;
use crate::{Error, Result};

impl Job<'_> {
    pub(super) async fn release_for_target(&self, target: &Candidate) -> Result<Release> {
        release_for_target(self.gh, self.app, target).await
    }
}

/// Shared by plans and execution: an asset must belong to a published release
/// whose tag identifies the selected commit, including pinned variants.
pub(crate) async fn release_for_target(
    gh: &GitHub,
    app: &TrackedApp,
    target: &Candidate,
) -> Result<Release> {
    let repo = app.repo_id()?;
    match gh.release_by_tag(&repo, &target.id).await {
        Ok(release) => return verify_release_commit(gh, app, release, target).await,
        Err(Error::RemoteNotFound(_)) => {}
        Err(error) => return Err(error),
    }
    let sha = target.sha.as_deref().unwrap_or(&target.id);
    if let Some(variant) = &app.require_github("release assets")?.options.variant
        && variant.reference.eq_ignore_ascii_case(sha)
        && let Some(tag) = &variant.release_tag
    {
        return verify_release_commit(gh, app, gh.release_by_tag(&repo, tag).await?, target).await;
    }
    let tags = gh.tags_page(&repo).await?;
    for tag in tags
        .items
        .iter()
        .filter(|tag| tag.commit.sha.eq_ignore_ascii_case(sha))
    {
        match gh.release_by_tag(&repo, &tag.name).await {
            Ok(release) => return verify_release_commit(gh, app, release, target).await,
            Err(Error::RemoteNotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    Err(Error::RemoteNotFound(format!(
        "no published release found for commit {sha}{}; choose a source build or an exact release tag",
        if tags.truncated {
            format!(
                " (tag scan stopped after {} newest tags; an older tag may hold it)",
                tags.items.len()
            )
        } else {
            String::new()
        }
    )))
}

async fn verify_release_commit(
    gh: &GitHub,
    app: &TrackedApp,
    release: Release,
    target: &Candidate,
) -> Result<Release> {
    if release.draft {
        return Err(Error::InvalidInput(
            "draft releases cannot be installed".into(),
        ));
    }
    if let Some(sha) = &target.sha {
        let actual = gh
            .commit(&app.repo_id()?, &format!("refs/tags/{}", release.tag_name))
            .await?;
        if !actual.sha.eq_ignore_ascii_case(sha) {
            return Err(Error::InvalidInput(
                "release tag does not identify the selected commit".into(),
            ));
        }
    }
    Ok(release)
}
