//! Additional GitHub origins and recipes, compatible with single-repository configs.
use serde::{Deserialize, Serialize};

use crate::repo::{RepoId, validate_app_id};
use crate::{AssetInstall, Install, Result, SourceInstall, Track};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryOption {
    pub repo: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantInfo {
    pub parent_id: String,
    /// The resolved commit, never a moving branch or an inferred version.
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubOptions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<RepositoryOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_build: Option<SourceInstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_asset: Option<AssetInstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantInfo>,
}

impl GithubOptions {
    pub(crate) fn validate(&self) -> Result<()> {
        let mut repos = std::collections::BTreeSet::new();
        for repository in &self.repositories {
            let repo = RepoId::parse(&repository.repo)?;
            if !repos.insert(repo.full_name().to_ascii_lowercase()) {
                return Err(crate::Error::InvalidInput(
                    "duplicate repository option".into(),
                ));
            }
            if repository.label.len() > 120 || repository.label.contains(char::is_control) {
                return Err(crate::Error::InvalidInput(
                    "repository label must be at most 120 bytes without control characters".into(),
                ));
            }
            crate::config::validate_remote(
                &Track::Commit {
                    branch: repository.branch.clone(),
                },
                repository.git_url.as_deref(),
            )
            .map_err(crate::Error::InvalidInput)?;
        }
        for install in self
            .source_build
            .clone()
            .map(Install::Source)
            .into_iter()
            .chain(self.release_asset.clone().map(Install::Asset))
        {
            crate::config::validate_install(&install).map_err(crate::Error::InvalidInput)?;
        }
        if let Some(variant) = &self.variant {
            validate_app_id(&variant.parent_id)?;
            if variant.reference.len() != 40
                || !variant.reference.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(crate::Error::InvalidInput(
                    "independent versions require a resolved 40-character commit".into(),
                ));
            }
        }
        Ok(())
    }
}
