//! Lazy commit browsing independent of installed/update state.
use crate::check::{commit_candidate, resolve_branch};
use crate::config::Track;
use crate::revision_history::{CommitPage, is_full_sha, validate_page};
use crate::{Error, Headway, Result};

impl Headway {
    /// Lists one page of the selected GitHub source's branch history.
    /// Without an anchor, page 1 resolves the branch's current tip. Reuse the
    /// returned anchor on all later pages to keep a stable history snapshot.
    ///
    /// # Errors
    /// Rejects non-GitHub sources, invalid pagination/anchors and unanchored
    /// later pages. GitHub repository, branch and page errors propagate.
    pub async fn list_commits(
        &self,
        id: &str,
        page: u32,
        per_page: u32,
        anchor: Option<&str>,
    ) -> Result<CommitPage> {
        validate_page(page, per_page)?;
        if anchor.is_some_and(|anchor| !is_full_sha(anchor)) {
            return Err(Error::InvalidInput(
                "Commit history anchor must be a full 40-character hexadecimal SHA.".into(),
            ));
        }
        if page > 1 && anchor.is_none() {
            return Err(Error::InvalidInput(
                "Later commit pages require the anchor returned by page 1.".into(),
            ));
        }
        let app = self.store().config()?.app(id)?.clone();
        let source = app.require_github("commit history")?;
        let repo = source.repo_id()?;
        let gh = self.github().await?;
        let configured_branch = if let Some(variant) = &source.options.variant {
            variant.branch.as_deref()
        } else {
            match &source.track {
                Track::Commit { branch } => branch.as_deref(),
                Track::Release { .. } | Track::Tag { .. } => None,
            }
        };
        let branch = resolve_branch(&gh, &repo, configured_branch).await?;
        let anchor = match anchor {
            Some(anchor) => anchor.to_ascii_lowercase(),
            None => gh.commit(&repo, &branch).await?.sha.to_ascii_lowercase(),
        };
        if !is_full_sha(&anchor) {
            return Err(Error::Parse {
                what: "GitHub branch tip".into(),
                message: "expected a full 40-character hexadecimal SHA".into(),
            });
        }
        let found = gh.commits_page(&repo, &anchor, page, per_page).await?;
        let next_page = if found.has_more {
            Some(page.checked_add(1).ok_or_else(|| {
                Error::InvalidInput("Commit page number cannot advance beyond u32::MAX.".into())
            })?)
        } else {
            None
        };
        Ok(CommitPage {
            repo: repo.full_name(),
            branch,
            anchor,
            page,
            per_page,
            commits: found.commits.iter().map(commit_candidate).collect(),
            has_more: found.has_more,
            next_page,
        })
    }
}

#[cfg(test)]
#[path = "commits_tests.rs"]
mod tests;
