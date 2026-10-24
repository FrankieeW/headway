use std::collections::BTreeMap;

use super::{BuildCandidate, Discovery};
use crate::error::Result;

pub(super) async fn collect(discovery: &mut Discovery<'_>) -> Result<()> {
    let Some(root) = discovery.files.get("Cargo.toml") else {
        return Ok(());
    };
    let Ok(manifest) = root.parse::<toml::Table>() else {
        return Ok(());
    };
    let members = manifest
        .get("workspace")
        .and_then(|workspace| {
            workspace
                .get("default-members")
                .or_else(|| workspace.get("members"))
        })
        .and_then(toml::Value::as_array);
    let paths: Vec<String> = members
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .filter(|path| safe_path(path))
        .take(8)
        .map(|path| format!("{}/Cargo.toml", path.trim_end_matches('/')))
        .collect();
    discovery.read("src/main.rs").await?;
    for path in paths {
        discovery.read(&path).await?;
        if let Some(parent) = path.strip_suffix("Cargo.toml") {
            discovery.read(&format!("{parent}src/main.rs")).await?;
        }
    }
    Ok(())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path.split('/').all(|part| part != "..")
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte))
}

pub(super) fn candidates(files: &BTreeMap<String, String>) -> Vec<BuildCandidate> {
    let mut candidates = Vec::new();
    for (file, text) in files
        .iter()
        .filter(|(file, _)| file.ends_with("Cargo.toml"))
    {
        let Ok(manifest) = text.parse::<toml::Table>() else {
            continue;
        };
        let parent = file.strip_suffix("Cargo.toml").unwrap_or("");
        let explicit: Vec<&str> = manifest
            .get("bin")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|bin| bin.get("name").and_then(toml::Value::as_str))
            .collect();
        let package = manifest.get("package");
        let bins = if explicit.is_empty()
            && files.contains_key(&format!("{parent}src/main.rs"))
            && package
                .and_then(|value| value.get("autobins"))
                .and_then(toml::Value::as_bool)
                != Some(false)
        {
            package
                .and_then(|value| value.get("name"))
                .and_then(toml::Value::as_str)
                .into_iter()
                .collect()
        } else {
            explicit
        };
        for bin in bins {
            // Cargo names become path components, never shell syntax.
            if !bin
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            {
                continue;
            }
            let build = if parent.is_empty() {
                "cargo build --release --locked".to_owned()
            } else {
                format!(
                    "cargo build --release --locked --manifest-path {} --bin {}",
                    crate::process::shell_quote(file),
                    crate::process::shell_quote(bin)
                )
            };
            let mut candidate = BuildCandidate::manifest(file, build);
            candidate.artifact = Some(format!("target/release/{bin}"));
            candidates.push(candidate);
        }
    }
    candidates
}
