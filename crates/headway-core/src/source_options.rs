//! Additional GitHub origins and recipes, compatible with single-repository configs.
use serde::{Deserialize, Serialize};

use crate::repo::{RepoId, validate_app_id};
use crate::{AssetInstall, Install, Result, SourceInstall, Track};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryOption {
    pub repo: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Saved when leaving this origin; restored when explicitly selecting it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<RepositorySettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySettings {
    pub track: Track,
    pub install: Install,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_build: Option<SourceInstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_asset: Option<AssetInstall>,
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
            if let Some(settings) = &repository.settings {
                settings.track.tag_regex()?;
                crate::config::validate_remote(&settings.track, repository.git_url.as_deref())
                    .map_err(crate::Error::InvalidInput)?;
                for install in std::iter::once(settings.install.clone())
                    .chain(settings.source_build.clone().map(Install::Source))
                    .chain(settings.release_asset.clone().map(Install::Asset))
                {
                    validate_install(&install)?;
                }
            }
        }
        for install in self
            .source_build
            .clone()
            .map(Install::Source)
            .into_iter()
            .chain(self.release_asset.clone().map(Install::Asset))
        {
            validate_install(&install)?;
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

/// Validates an install entry, including the asset glob that selects its payload.
fn validate_install(install: &Install) -> Result<()> {
    crate::config::validate_install(install).map_err(crate::Error::InvalidInput)?;
    if let Install::Asset(asset) = install
        && let Some(pattern) = &asset.pattern
    {
        crate::asset::validate_pattern(pattern)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_pattern(pattern: &str) -> GithubOptions {
        GithubOptions {
            release_asset: Some(AssetInstall {
                pattern: Some(pattern.to_owned()),
                ..AssetInstall::default()
            }),
            ..GithubOptions::default()
        }
    }

    #[test]
    fn asset_patterns_are_validated() {
        assert!(with_pattern("*macos*.zip").validate().is_ok());
        let long = "z".repeat(257);
        for pattern in ["", "bad\npattern", long.as_str()] {
            assert!(with_pattern(pattern).validate().is_err(), "{pattern:?}");
        }
    }
}
