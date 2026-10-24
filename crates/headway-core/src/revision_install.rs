//! A one-off recipe choice for an exact revision, independent of the tracking policy.
use crate::{AssetInstall, Candidate, SourceInstall};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionMethod {
    #[default]
    Configured,
    Source,
    Asset,
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
