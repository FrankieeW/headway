use std::collections::BTreeMap;

use serde::Deserialize;

use super::BuildCandidate;
use crate::config::AppKind;
use crate::github::ContentEntry;
use crate::presets::{TAURI_LOCAL_CONFIG, TAURI_LOCAL_SIGNING};

pub(super) const TAURI_LOCAL_ID: &str = "manifest:tauri-local";

#[derive(Deserialize)]
struct Package {
    #[serde(default)]
    scripts: BTreeMap<String, String>,
    #[serde(rename = "packageManager")]
    package_manager: Option<String>,
}

#[derive(Deserialize)]
struct TauriConfig {
    #[serde(rename = "$schema")]
    schema: Option<String>,
    // Tauri 2 requires a top-level identifier; Tauri 1 kept it under `tauri.bundle`.
    identifier: Option<String>,
    #[serde(default)]
    build: TauriBuild,
}

#[derive(Default, Deserialize)]
struct TauriBuild {
    #[serde(rename = "beforeBuildCommand")]
    before_build_command: Option<String>,
}

pub(super) fn candidates(
    files: &BTreeMap<String, String>,
    entries: &[ContentEntry],
) -> Vec<BuildCandidate> {
    let mut out = Vec::new();
    if let Some(text) = files.get("package.json") {
        let tauri_artifact = files.contains_key("src-tauri/tauri.conf.json").then(|| {
            let workspace = files
                .get("Cargo.toml")
                .is_some_and(|text| crate::presets::cargo_workspace_contains(text, "src-tauri"));
            if workspace {
                "target/release/bundle/macos/*.app"
            } else {
                "src-tauri/target/release/bundle/macos/*.app"
            }
        });
        let tauri_config = files
            .get("src-tauri/tauri.conf.json")
            .and_then(|text| serde_json::from_str::<TauriConfig>(text).ok())
            .filter(|config| {
                config.identifier.is_some()
                    || config
                        .schema
                        .as_ref()
                        .is_some_and(|schema| schema.trim_end_matches('/').ends_with("/config/2"))
            });
        out.extend(package(
            text,
            entries,
            tauri_artifact,
            tauri_config.as_ref(),
        ));
    }
    if files
        .get("main.go")
        .is_some_and(|text| text.lines().any(|line| line.trim() == "package main"))
    {
        let mut candidate = BuildCandidate::manifest(
            "main.go",
            "go build -trimpath -o .headway-out/tool .".into(),
        );
        candidate.artifact = Some(".headway-out/tool".into());
        out.push(candidate);
    }
    if files
        .get("Package.swift")
        .is_some_and(|text| text.contains(".executableTarget(") || text.contains(".executable("))
    {
        out.push(BuildCandidate::manifest(
            "Package.swift",
            "swift build -c release".into(),
        ));
    }
    for file in ["Makefile", "makefile", "GNUmakefile"] {
        if let Some(text) = files.get(file) {
            for target in ["build", "all"] {
                if text
                    .lines()
                    .any(|line| line.starts_with(&format!("{target}:")))
                {
                    out.push(BuildCandidate::manifest(file, format!("make {target}")));
                    break;
                }
            }
        }
    }
    if files
        .get("CMakeLists.txt")
        .is_some_and(|text| text.contains("add_executable("))
    {
        out.push(BuildCandidate::manifest("CMakeLists.txt", "cmake -S . -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build --config Release".into()));
    }
    out
}

fn package(
    text: &str,
    entries: &[ContentEntry],
    tauri_artifact: Option<&str>,
    tauri_config: Option<&TauriConfig>,
) -> Vec<BuildCandidate> {
    let Ok(package) = serde_json::from_str::<Package>(text) else {
        return Vec::new();
    };
    let has = |file| entries.iter().any(|entry| entry.name == file);
    let declared = package
        .package_manager
        .as_deref()
        .and_then(|value| value.split('@').next());
    let manager = match declared {
        Some("pnpm") => "pnpm",
        Some("yarn") => "yarn",
        Some("bun") => "bun",
        Some("npm") => "npm",
        Some(_) => return Vec::new(),
        None if has("pnpm-lock.yaml") => "pnpm",
        None if has("yarn.lock") => "yarn",
        None if has("bun.lock") || has("bun.lockb") => "bun",
        None => "npm",
    };
    let install = match manager {
        "pnpm" if has("pnpm-lock.yaml") => "pnpm install --frozen-lockfile",
        "pnpm" => "pnpm install",
        "yarn" => "yarn install",
        "bun" => "bun install",
        _ if has("package-lock.json") => "npm ci",
        _ => "npm install",
    };
    let local_eligible = local_tauri(&package, manager, tauri_config);
    package.scripts.iter().filter_map(|(name, script)| {
        let production = matches!(name.as_str(), "build" | "compile" | "bundle" | "package") || name.starts_with("build:") || script.contains("tauri build")
            || script.contains("electron-builder") || script.contains("vite build");
        let tauri_cli = name == "tauri" && script == "tauri";
        if !(production || tauri_cli) || !super::readme::safe(script) || !super::readme::safe(name) { return None; }
        let mut candidate = BuildCandidate::manifest("package.json", format!("{install} && {manager} run {}", crate::process::shell_quote(name)));
        let local = tauri_cli && local_eligible;
        if tauri_cli {
            let args = if manager == "npm" { "-- build --bundles app" } else { "build --bundles app" };
            candidate.build = if local {
                format!("{install} && {TAURI_LOCAL_SIGNING} {manager} run tauri {args} {TAURI_LOCAL_CONFIG}")
            } else {
                format!("{install} && {manager} run tauri {args}")
            };
        }
        if script.contains("tauri") {
            candidate.artifact = tauri_artifact.map(str::to_owned);
        }
        if script.contains("tauri") || script.contains("electron-builder") {
            candidate.kind = AppKind::App;
        }
        candidate.reason = format!("package.json declares `{name}` using {manager}; inspect the output path before installing.");
        if local {
            candidate.id = TAURI_LOCAL_ID.into();
            candidate.reason.push_str(" Local Tauri 2 build defaults to ad-hoc app signing and skips updater artifact signing; project files are not modified.");
        }
        Some(candidate)
    }).collect()
}

/// Bypassing a build script is safe only when it is a plain Tauri wrapper or Tauri
/// invokes it itself, preserving frontend preparation and npm lifecycle hooks.
fn local_tauri(package: &Package, manager: &str, config: Option<&TauriConfig>) -> bool {
    let Some(config) = config else {
        return false;
    };
    package.scripts.get("build").is_none_or(|build| {
        if build.contains("tauri") {
            matches!(
                build.as_str(),
                "tauri build"
                    | "pnpm tauri build"
                    | "pnpm run tauri build"
                    | "npm run tauri -- build"
                    | "npm run tauri build"
                    | "yarn tauri build"
                    | "yarn run tauri build"
                    | "bun tauri build"
                    | "bun run tauri build"
            ) && !package.scripts.contains_key("prebuild")
                && !package.scripts.contains_key("postbuild")
        } else {
            config
                .build
                .before_build_command
                .as_deref()
                .is_some_and(|command| {
                    let words: Vec<_> = command.split_whitespace().collect();
                    words == [manager, "run", "build"]
                        || (manager != "npm" && words == [manager, "build"])
                })
        }
    })
}
