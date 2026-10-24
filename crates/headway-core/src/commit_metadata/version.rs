//! Version declarations belong to a commit, independently of tags and releases.
use serde::Deserialize;

use crate::github::{BoundedFileText, GitHub};
use crate::repo::RepoId;

#[derive(Clone)]
pub(super) struct ManifestLookup {
    pub version: Option<String>,
    pub notes: Vec<String>,
    pub cacheable: bool,
}

#[derive(Deserialize)]
struct CargoManifest {
    package: Option<Package>,
    workspace: Option<Workspace>,
}

#[derive(Deserialize)]
struct Workspace {
    package: Option<Package>,
}

#[derive(Deserialize)]
struct Package {
    version: Option<VersionSpec>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum VersionSpec {
    Literal(String),
    Inherited {
        #[serde(rename = "workspace")]
        _workspace: bool,
    },
}

impl Package {
    fn literal_version(self) -> Option<String> {
        match self.version? {
            VersionSpec::Literal(value) => is_version(&value).then_some(value),
            VersionSpec::Inherited { .. } => None,
        }
    }
}

pub(super) async fn fetch(gh: &GitHub, repo: &RepoId, sha: &str) -> ManifestLookup {
    let mut lookup = ManifestLookup {
        version: None,
        notes: Vec::new(),
        cacheable: true,
    };
    for file in ["Cargo.toml", "package.json"] {
        let text = match gh.file_text_at_bounded(repo, file, sha, 64 * 1024).await {
            Ok(BoundedFileText::Text(text)) => text,
            Ok(BoundedFileText::Missing) => continue,
            Ok(BoundedFileText::TooLarge) => {
                lookup.notes.push(format!(
                    "Skipped {file} at {sha}: version manifest exceeds 65536 bytes."
                ));
                continue;
            }
            Err(error) => {
                lookup.cacheable = false;
                lookup.notes.push(format!(
                    "Version manifest {file} at {sha} unavailable: {error}"
                ));
                continue;
            }
        };
        let parsed = if file == "Cargo.toml" {
            toml::from_str::<CargoManifest>(&text)
                .map(|manifest| {
                    manifest
                        .workspace
                        .and_then(|workspace| workspace.package)
                        .and_then(Package::literal_version)
                        .or_else(|| manifest.package.and_then(Package::literal_version))
                })
                .map_err(|error| error.to_string())
        } else {
            serde_json::from_str::<Package>(&text)
                .map(Package::literal_version)
                .map_err(|error| error.to_string())
        };
        match parsed {
            Ok(Some(version)) => {
                lookup.version = Some(version);
                break;
            }
            Ok(None) => {}
            Err(error) => lookup.notes.push(format!(
                "Could not parse version in {file} at {sha}: {error}"
            )),
        }
    }
    lookup
}

pub(super) fn is_version(value: &str) -> bool {
    let core = value.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<_> = core.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
}
