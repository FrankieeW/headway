//! Bounded commit-history pages anchored to an immutable Git commit.
use serde::{Deserialize, Serialize};

use crate::{Candidate, Error, Result};

/// One newest-first page; pass `anchor` back unchanged for subsequent pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitPage {
    pub repo: String,
    pub branch: String,
    pub anchor: String,
    pub page: u32,
    pub per_page: u32,
    pub commits: Vec<Candidate>,
    pub has_more: bool,
    pub next_page: Option<u32>,
}

pub(crate) fn validate_page(page: u32, per_page: u32) -> Result<()> {
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(Error::InvalidInput(
            "Pagination requires page >= 1 and per_page between 1 and 100.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn is_full_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}
