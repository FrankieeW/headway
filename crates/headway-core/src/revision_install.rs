//! A one-off recipe choice for an exact revision, independent of the tracking policy.
use crate::{
    AssetInstall, Candidate, Error, GithubSource, Install, InstalledRef, Result, SourceInstall,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionMethod {
    #[default]
    Configured,
    Source,
    Asset,
}

impl RevisionMethod {
    /// The recipe this method installs with for one run, placed at `dest`.
    ///
    /// # Errors
    /// A source build needs a build recipe, `Configured` must already be resolved
    /// to a concrete method, and the resulting recipe must be valid.
    pub(crate) fn recipe(
        self,
        source_build: Option<SourceInstall>,
        release_asset: AssetInstall,
        dest: Option<String>,
    ) -> Result<Install> {
        let install = match self {
            Self::Source => Install::Source(SourceInstall {
                dest,
                ..source_build.ok_or_else(|| {
                    Error::InvalidInput(
                        "configure a source build command and artifact before building this revision"
                            .into(),
                    )
                })?
            }),
            Self::Asset => Install::Asset(AssetInstall {
                dest,
                ..release_asset
            }),
            Self::Configured => {
                return Err(Error::InvalidInput("unresolved install method".into()));
            }
        };
        crate::config::validate_install(&install).map_err(Error::InvalidInput)?;
        Ok(install)
    }
}

impl GithubSource {
    /// The source-build recipe: the saved one, or the alternate kept for one-off builds.
    pub(crate) fn source_recipe(&self) -> Option<SourceInstall> {
        match &self.install {
            Install::Source(source) => Some(source.clone()),
            Install::Asset(_) => self.options.source_build.clone(),
        }
    }

    /// The release-asset recipe: the saved one, or the alternate kept for one-off downloads.
    pub(crate) fn asset_recipe(&self) -> AssetInstall {
        match &self.install {
            Install::Asset(asset) => asset.clone(),
            Install::Source(_) => self.options.release_asset.clone().unwrap_or_default(),
        }
    }

    /// Where a one-off recipe installs: where the saved recipe installs, or over
    /// the installed payload when the saved recipe names no destination.
    ///
    /// An alternate recipe never brings its own destination. Recovery checks every
    /// run against the saved recipe, and a one-off method replaces the installed
    /// payload instead of adding a second copy somewhere else.
    pub(crate) fn one_off_destination(&self, installed: Option<&InstalledRef>) -> Option<String> {
        match &self.install {
            Install::Source(source) => source.dest.clone(),
            Install::Asset(asset) => asset.dest.clone(),
        }
        .or_else(|| installed.and_then(|installed| installed.path.clone()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionRequest {
    pub reference: String,
    /// A verified tag association may be carried alongside an immutable SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_tag: Option<String>,
    #[serde(default)]
    pub method: RevisionMethod,
    #[serde(default)]
    pub source_build: Option<SourceInstall>,
    #[serde(default)]
    pub release_asset: Option<AssetInstall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionOptions {
    pub reference: String,
    pub commit: String,
    pub candidate: Candidate,
    pub release_tag: Option<String>,
    pub release_url: Option<String>,
    pub can_download: bool,
    pub source_build: Option<SourceInstall>,
    pub release_asset: Option<AssetInstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<crate::github::PullRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub revision: RevisionRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOutcome {
    pub app_id: String,
    pub pid: u32,
    pub path: String,
}
