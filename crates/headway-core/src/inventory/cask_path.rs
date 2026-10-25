//! Locate cask installations using only explicit absolute artifact targets.
use crate::source::absolute_install_path;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct Artifact {
    #[serde(default)]
    target: Option<Value>,
    #[serde(flatten)]
    body: serde_json::Map<String, Value>,
}

pub(super) fn locate(token: &str, artifacts: &[Artifact]) -> Option<String> {
    for kind in ["app", "suite"] {
        if has_kind(artifacts, kind) {
            return unique(absolute_targets(artifacts, kind));
        }
    }
    let binaries = absolute_targets(artifacts, "binary");
    let short = token.rsplit('/').next().unwrap_or(token);
    let primary = binaries
        .iter()
        .filter(|path| {
            std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                == Some(short)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !primary.is_empty() {
        return unique(primary);
    }
    if !binaries.is_empty() {
        return unique(binaries);
    }
    for kind in [
        "font",
        "qlplugin",
        "mdimporter",
        "prefpane",
        "colorpicker",
        "dictionary",
        "screen_saver",
        "service",
        "input_method",
        "artifact",
        "manpage",
        "bash_completion",
        "zsh_completion",
        "fish_completion",
    ] {
        if has_kind(artifacts, kind) {
            return unique(absolute_targets(artifacts, kind));
        }
    }
    None
}

fn has_kind(artifacts: &[Artifact], kind: &str) -> bool {
    artifacts
        .iter()
        .any(|artifact| artifact.body.contains_key(kind))
}

fn unique(mut paths: Vec<String>) -> Option<String> {
    if paths.len() == 1 { paths.pop() } else { None }
}

fn absolute_targets(artifacts: &[Artifact], kind: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for artifact in artifacts
        .iter()
        .filter(|artifact| artifact.body.contains_key(kind))
    {
        let direct = artifact
            .target
            .as_ref()
            .and_then(Value::as_str)
            .and_then(absolute_install_path);
        if let Some(path) = direct {
            if !paths.contains(&path) {
                paths.push(path);
            }
            continue;
        }
        if let Some(parts) = artifact.body.get(kind).and_then(Value::as_array) {
            for part in parts {
                if let Some(path) = part
                    .get("target")
                    .and_then(Value::as_str)
                    .and_then(absolute_install_path)
                    && !paths.contains(&path)
                {
                    paths.push(path);
                }
            }
        }
    }
    paths
}
