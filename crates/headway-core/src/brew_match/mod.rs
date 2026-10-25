//! Read-only Homebrew/GitHub suggestions: metadata URLs prove identity; names and search only hint.
#[cfg(test)]
mod algorithm_tests;
#[cfg(test)]
mod bounds_tests;
#[cfg(test)]
mod metadata_tests;
#[cfg(test)]
mod tests;

use crate::{Error, ErrorInfo};
use serde::{Deserialize, Serialize};

pub(crate) mod evidence;
pub(crate) mod metadata;
mod probe;
mod search;
mod targets;
pub(crate) mod to_brew;
pub(crate) mod to_github;
mod urls;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSide {
    Github,
    Brew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchEvidenceKind {
    SourceUrl,
    HeadUrl,
    DownloadUrl,
    Homepage,
    HomepageMatch,
    Name,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MatchEvidence {
    pub kind: MatchEvidenceKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryMatch {
    pub repo: String,
    pub html_url: String,
    pub confidence: MatchConfidence,
    pub evidence: Vec<MatchEvidence>,
    pub description: Option<String>,
    pub stars: Option<u64>,
    pub tracked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageMatch {
    pub package: String,
    pub cask: bool,
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub installed: Option<String>,
    pub repo: String,
    pub confidence: MatchConfidence,
    pub evidence: Vec<MatchEvidence>,
    pub tracked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchError {
    pub source: String,
    pub error: ErrorInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMatches {
    pub query: String,
    pub from: MatchSide,
    pub repositories: Vec<RepositoryMatch>,
    pub packages: Vec<PackageMatch>,
    pub notes: Vec<String>,
    pub errors: Vec<MatchError>,
}

impl SourceMatches {
    fn new(query: String, from: MatchSide) -> Self {
        Self {
            query,
            from,
            repositories: Vec::new(),
            packages: Vec::new(),
            notes: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn error(&mut self, source: &str, error: &Error) {
        self.errors.push(MatchError {
            source: source.into(),
            error: error.into(),
        });
    }
}
