//! Read-only discovery of installed package-manager entries.

use serde::{Deserialize, Serialize};

use crate::config::{AppKind, TrackedApp};
use crate::error::{Error, ErrorInfo, Result};
use crate::process::Cmd;
use crate::shell::ShellEnv;
use crate::source::Source;
use crate::{CancellationToken, probe};

mod cask_path;
mod parse;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCandidate {
    pub key: String,
    pub app: TrackedApp,
    pub installed_version: Option<String>,
    pub already_tracked: bool,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InventoryReport {
    pub candidates: Vec<InventoryCandidate>,
    pub errors: Vec<InventoryError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryError {
    pub source: String,
    pub error: ErrorInfo,
}

impl InventoryReport {
    fn error(&mut self, source: &str, error: &Error) {
        self.errors.push(InventoryError {
            source: source.into(),
            error: error.into(),
        });
    }
}

pub(crate) async fn discover(sources: &[String], env: &ShellEnv) -> Result<InventoryReport> {
    let names = if sources.is_empty() {
        vec!["brew", "npm"]
    } else {
        sources.iter().map(String::as_str).collect()
    };
    for name in &names {
        if !matches!(*name, "brew" | "npm") {
            return Err(Error::InvalidInput(format!(
                "unsupported inventory source `{name}`; use brew or npm"
            )));
        }
    }
    let mut report = InventoryReport::default();
    let mut visited = std::collections::BTreeSet::new();
    for name in names {
        if !visited.insert(name) {
            continue;
        }
        let cmd = match name {
            "brew" => Cmd::new("brew", env)
                .args(["info", "--installed", "--json=v2"])
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env("HOMEBREW_NO_ANALYTICS", "1"),
            "npm" => Cmd::new("npm", env).args(["ls", "--global", "--depth=0", "--json"]),
            _ => {
                return Err(Error::InvalidInput(format!(
                    "unsupported inventory source `{name}`"
                )));
            }
        };
        match probe::capture(&CancellationToken::new(), &cmd).await {
            Ok(out) => {
                if !out.success() {
                    report.error(name, &probe::failed(&cmd, &out));
                }
                // npm can return useful entries together with ELSPROBLEMS and exit 1.
                let parsed = match name {
                    "brew" => parse::brew(&out.stdout),
                    "npm" => parse::npm(&out.stdout),
                    _ => {
                        return Err(Error::InvalidInput(format!(
                            "unsupported inventory source `{name}`"
                        )));
                    }
                };
                match parsed {
                    Ok(candidates) => report.candidates.extend(candidates),
                    Err(error) => report.error(name, &error),
                }
            }
            Err(error) => report.error(name, &error),
        }
    }
    report.candidates.sort_by(|a, b| a.key.cmp(&b.key));
    let mut ids = std::collections::BTreeSet::new();
    for candidate in &mut report.candidates {
        let base = candidate.app.id.clone();
        let mut suffix = 1_u64;
        while !ids.insert(candidate.app.id.clone()) {
            suffix = suffix.saturating_add(1);
            candidate.app.id = format!("{}-{suffix}", base.chars().take(42).collect::<String>());
        }
    }
    Ok(report)
}

pub(crate) fn candidate(
    source: Source,
    name: String,
    installed_version: Option<String>,
) -> InventoryCandidate {
    use sha2::{Digest as _, Sha256};
    let key = source.label();
    let digest = format!("{:x}", Sha256::digest(key.as_bytes()));
    let slug: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(43)
        .collect();
    let kind = match &source {
        Source::Brew(brew) if brew.cask => AppKind::App,
        _ => AppKind::Cli,
    };
    InventoryCandidate {
        app: TrackedApp {
            id: format!("{slug}-{}", &digest[..20]),
            name,
            kind,
            auto_update: false,
            update_policy: crate::config::UpdatePolicy::default(),
            source,
        },
        key,
        installed_version,
        already_tracked: false,
        conflicts: Vec::new(),
    }
}
