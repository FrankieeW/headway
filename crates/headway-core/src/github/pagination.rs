//! Bounded pagination for list endpoints.
//!
//! Only page numbers Headway constructs itself are requested: a server-provided
//! `Link` header is advisory (`rel="next"` means "another page exists"), never a URL
//! to follow. Every list has a hard page cap, so a wrong or hostile response cannot
//! turn one check into an unbounded crawl.
//!
//! The caps truncate long lists. The plain accessors ([`GitHub::tags`],
//! [`GitHub::releases`], [`GitHub::branches`]) keep their old "return the items"
//! shape; each `_page` variant returns [`BoundedList`] so callers that need a
//! complete answer (for example mapping a commit SHA back to a release tag) can
//! detect truncation and fail with an actionable message instead of treating a
//! partial list as complete.

use reqwest::header::LINK;
use serde::de::DeserializeOwned;

use super::commits_page::has_next_relation;
use super::http::{API_DEADLINE, JSON, MAX_JSON_BYTES, bounded_body, deadline};
use super::{Branch, GitHub, Release, Tag};
use crate::error::{Error, Result};
use crate::repo::RepoId;

const ITEMS_PER_PAGE: u32 = 100;
/// Absolute ceiling across pages, even if the server keeps claiming another page.
/// With the per-page bound below this is unreachable for the current caps; it stays
/// as an independent stop condition.
const MAX_ITEMS: usize = 1000;
/// Tags are scanned to map commits back to releases; older tags matter for `--to`.
const TAG_PAGES: u32 = 5;
const RELEASE_PAGES: u32 = 3;
const BRANCH_PAGES: u32 = 2;

/// A bounded list plus whether the scan stopped before the server ran out of pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedList<T> {
    pub items: Vec<T>,
    /// `true` when Headway hit its page cap while the server still advertised more.
    pub truncated: bool,
}

impl<T> BoundedList<T> {
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
}

impl GitHub {
    /// First [`BRANCH_PAGES`] pages of branches (≤200). Use the `_page` variant to
    /// know whether the list is complete.
    pub async fn branches(&self, repo: &RepoId) -> Result<Vec<Branch>> {
        Ok(self.branches_page(repo).await?.items)
    }

    /// Branches, with an explicit truncation flag.
    pub async fn branches_page(&self, repo: &RepoId) -> Result<BoundedList<Branch>> {
        self.pages(
            &format!("/repos/{repo}/branches"),
            &format!("branches of {repo}"),
            BRANCH_PAGES,
        )
        .await
    }

    /// First [`RELEASE_PAGES`] pages of releases (≤300), in GitHub's order (newest
    /// first). Use the `_page` variant when completeness matters.
    pub async fn releases(&self, repo: &RepoId) -> Result<Vec<Release>> {
        Ok(self.releases_page(repo).await?.items)
    }

    /// Releases, with an explicit truncation flag.
    pub async fn releases_page(&self, repo: &RepoId) -> Result<BoundedList<Release>> {
        self.pages(
            &format!("/repos/{repo}/releases"),
            &format!("releases of {repo}"),
            RELEASE_PAGES,
        )
        .await
    }

    /// First [`TAG_PAGES`] pages of tags (≤500), in GitHub's order.
    ///
    /// This is a bounded scan, not the complete tag set: repositories with more tags
    /// are silently truncated here. Callers that treat the list as complete — such as
    /// resolving a commit SHA to the release tag that contains it — must use
    /// [`GitHub::tags_page`] and report truncation instead of a false "not found".
    pub async fn tags(&self, repo: &RepoId) -> Result<Vec<Tag>> {
        Ok(self.tags_page(repo).await?.items)
    }

    /// Tags, with an explicit truncation flag.
    pub async fn tags_page(&self, repo: &RepoId) -> Result<BoundedList<Tag>> {
        self.pages(
            &format!("/repos/{repo}/tags"),
            &format!("tags of {repo}"),
            TAG_PAGES,
        )
        .await
    }

    async fn pages<T: DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
        max_pages: u32,
    ) -> Result<BoundedList<T>> {
        let per_page = usize::try_from(ITEMS_PER_PAGE).unwrap_or(usize::MAX);
        let mut items: Vec<T> = Vec::new();
        let mut truncated = false;
        for page in 1..=max_pages {
            let endpoint = format!("{path}?per_page={ITEMS_PER_PAGE}&page={page}");
            let (mut page_items, has_more) = self.page(&endpoint, what).await?;
            let full_page = page_items.len() >= per_page;
            items.append(&mut page_items);
            if !has_more || !full_page {
                break;
            }
            if page == max_pages || items.len() >= MAX_ITEMS {
                truncated = true;
                break;
            }
        }
        Ok(BoundedList { items, truncated })
    }

    async fn page<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        what: &str,
    ) -> Result<(Vec<T>, bool)> {
        let label = format!("GitHub request for {what}");
        let read = async {
            let response = self.send(|| self.request(endpoint, JSON), what).await?;
            let has_more = response
                .headers()
                .get_all(LINK)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .any(has_next_relation);
            let bytes = bounded_body(response, MAX_JSON_BYTES, what).await?;
            let items: Vec<T> = serde_json::from_slice(&bytes)
                .map_err(|error| Error::parse(format!("GitHub response for {what}"), &error))?;
            let per_page = usize::try_from(ITEMS_PER_PAGE).unwrap_or(usize::MAX);
            if items.len() > per_page {
                return Err(Error::Parse {
                    what: format!("GitHub response for {what}"),
                    message: format!(
                        "pagination page returned {} items, more than the requested {ITEMS_PER_PAGE}",
                        items.len()
                    ),
                });
            }
            Ok((items, has_more))
        };
        deadline(API_DEADLINE, &label, read).await
    }
}
