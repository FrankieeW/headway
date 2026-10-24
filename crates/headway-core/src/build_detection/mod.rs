//! Bounded, evidence-backed suggestions; no repository commands are executed here.
mod cargo;
pub(crate) mod fallback;
mod manifests;
mod readme;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::AppKind;
use crate::error::{Error, Result};
use crate::github::{BoundedFileText, ContentEntry, GitHub};
use crate::repo::RepoId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildCandidateSource {
    Readme,
    Manifest,
    Preset,
}

/// An editable suggestion, never an instruction executed by inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCandidate {
    pub id: String,
    pub source: BuildCandidateSource,
    pub file: String,
    pub line: Option<u32>,
    pub build: String,
    pub workdir: Option<String>,
    pub artifact: Option<String>,
    pub kind: AppKind,
    pub reason: String,
    pub recommended: bool,
}

impl BuildCandidate {
    pub(super) fn manifest(file: &str, build: String) -> Self {
        Self {
            id: format!("manifest:{file}:{build}"),
            source: BuildCandidateSource::Manifest,
            file: file.to_owned(),
            line: None,
            build,
            workdir: None,
            artifact: None,
            kind: AppKind::Cli,
            reason: "Build declared by repository source; review command and output before saving."
                .into(),
            recommended: false,
        }
    }
}

pub(crate) struct Discovery<'a> {
    gh: &'a GitHub,
    repo: &'a RepoId,
    pub files: BTreeMap<String, String>,
    pub notes: Vec<String>,
    requests: usize,
    bytes: usize,
}

impl<'a> Discovery<'a> {
    pub fn new(gh: &'a GitHub, repo: &'a RepoId) -> Self {
        Self {
            gh,
            repo,
            files: BTreeMap::new(),
            notes: Vec::new(),
            requests: 0,
            bytes: 0,
        }
    }

    pub async fn read(&mut self, path: &str) -> Result<()> {
        if self.files.contains_key(path) || self.requests >= 24 || self.bytes >= 512 * 1024 {
            return Ok(());
        }
        self.requests += 1;
        let limit = (128 * 1024).min(512 * 1024 - self.bytes);
        match self.gh.file_text_bounded(self.repo, path, limit).await {
            Ok(BoundedFileText::Text(text)) => {
                self.bytes += text.len();
                self.files.insert(path.to_owned(), text);
            }
            Ok(BoundedFileText::TooLarge) => {
                self.bytes += limit;
                self.notes.push(format!(
                    "Skipped oversized optional file `{path}` (limit {limit} bytes)."
                ));
            }
            Ok(BoundedFileText::Missing) => {}
            Err(err @ (Error::Auth(_) | Error::RateLimited { .. })) => return Err(err),
            Err(err) => {
                self.bytes += limit;
                self.notes
                    .push(format!("Could not inspect optional file `{path}`: {err}"));
            }
        }
        Ok(())
    }

    pub async fn collect(&mut self, entries: &[ContentEntry]) -> Result<()> {
        // Read README independently of root ordering and before workspace traversal.
        for path in ["README.md", "readme.md", "README", "README.rst"] {
            if entries.iter().any(|entry| entry.name == path) {
                self.read(path).await?;
            }
        }
        for path in [
            "Cargo.toml",
            "package.json",
            "go.mod",
            "Package.swift",
            "Makefile",
            "makefile",
            "GNUmakefile",
            "CMakeLists.txt",
        ] {
            if entries.iter().any(|entry| entry.name == path) {
                self.read(path).await?;
            }
        }
        if entries
            .iter()
            .any(|entry| entry.name == "src-tauri" && entry.kind == "dir")
        {
            self.read("src-tauri/tauri.conf.json").await?;
        }
        if self.files.contains_key("go.mod") {
            self.read("main.go").await?;
        }
        cargo::collect(self).await
    }

    pub fn candidates(&self, entries: &[ContentEntry]) -> Vec<BuildCandidate> {
        let mut candidates = cargo::candidates(&self.files);
        candidates.extend(manifests::candidates(&self.files, entries));
        let source = candidates.clone();
        for (file, text) in &self.files {
            if file.to_ascii_lowercase().starts_with("readme") {
                candidates.extend(readme::candidates(file, text, &source));
            }
        }
        candidates
    }
}

pub(crate) fn prioritize(candidates: &mut Vec<BuildCandidate>, known_self: bool) {
    candidates.sort_by_key(|candidate| {
        (
            !installable(candidate),
            match candidate.source {
                BuildCandidateSource::Preset if known_self => 0,
                BuildCandidateSource::Readme if candidate.recommended => 1,
                BuildCandidateSource::Manifest if candidate.kind == AppKind::App => 2,
                BuildCandidateSource::Preset if candidate.kind == AppKind::App => 3,
                BuildCandidateSource::Manifest => 4,
                BuildCandidateSource::Preset => 5,
                BuildCandidateSource::Readme => 6,
            },
        )
    });
    let mut seen = std::collections::BTreeSet::new();
    candidates
        .retain(|candidate| seen.insert((candidate.build.clone(), candidate.workdir.clone())));
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let ready = installable(candidate);
        candidate.recommended = index == 0 && ready;
        if !ready {
            candidate.reason.push_str(" Build-only suggestion: set an artifact path before saving, or explicitly replace this with a command that installs the program itself.");
        }
    }
}

fn installable(candidate: &BuildCandidate) -> bool {
    candidate
        .artifact
        .as_ref()
        .is_some_and(|path| !path.trim().is_empty())
        || (candidate.source == BuildCandidateSource::Preset
            && candidate
                .id
                .strip_prefix("preset:")
                .and_then(crate::presets::find)
                .is_some_and(|preset| {
                    preset.artifact.is_none()
                        && !preset.build.is_empty()
                        && preset.build == candidate.build
                }))
}

#[cfg(test)]
mod tests;
