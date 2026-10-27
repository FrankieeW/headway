//! Build presets for source installs, and detecting which one a repository needs.

use serde::Serialize;

use crate::config::AppKind;
use crate::github::ContentEntry;

// Keep the static preset and detected command on the same local signing policy.
macro_rules! local_tauri_flags {
    ($signing:literal, $config:literal) => {
        pub(crate) const TAURI_LOCAL_SIGNING: &str = $signing;
        pub(crate) const TAURI_LOCAL_CONFIG: &str = $config;
        const TAURI_LOCAL_BUILD: &str = concat!(
            "pnpm install --frozen-lockfile && ",
            $signing,
            " pnpm tauri build --bundles app ",
            $config
        );
    };
}

local_tauri_flags!(
    "if [ -z \"${APPLE_SIGNING_IDENTITY:-}\" ]; then export APPLE_SIGNING_IDENTITY=-; unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_KEY_PATH APPLE_API_ISSUER; fi &&",
    "--config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'"
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: AppKind,
    /// May contain `{bin}` (binary name) and `{name}` (app / scheme name).
    pub build: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<&'static str>,
    pub description: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "headway-app",
        label: "Headway app",
        kind: AppKind::App,
        build: "./scripts/build-headway.sh app",
        artifact: Some(".headway-out/Headway.app"),
        description: "Headway desktop app with its bundled CLI, built for this Mac.",
    },
    Preset {
        id: "headway-cli",
        label: "Headway CLI",
        kind: AppKind::Cli,
        build: "./scripts/build-headway.sh cli",
        artifact: Some(".headway-out/headway"),
        description: "Headway CLI built for this Mac.",
    },
    Preset {
        id: "cargo",
        label: "Cargo",
        kind: AppKind::Cli,
        build: "cargo build --release --locked",
        artifact: Some("target/release/{bin}"),
        description: "Rust binary: cargo build, then install target/release/<bin>.",
    },
    Preset {
        id: "cargo-install",
        label: "cargo install",
        kind: AppKind::Cli,
        build: "cargo install --path . --locked --force",
        artifact: None,
        description: "Let cargo install the binary into ~/.cargo/bin.",
    },
    Preset {
        id: "go",
        label: "Go",
        kind: AppKind::Cli,
        build: "go build -trimpath -o .headway-out/{bin} .",
        artifact: Some(".headway-out/{bin}"),
        description: "Go module: go build into .headway-out/<bin>.",
    },
    Preset {
        id: "swiftpm",
        label: "Swift Package",
        kind: AppKind::Cli,
        build: "swift build -c release --disable-sandbox",
        artifact: Some(".build/release/{bin}"),
        description: "SwiftPM executable: swift build -c release.",
    },
    Preset {
        id: "xcode",
        label: "Xcode app",
        kind: AppKind::App,
        build: "xcodebuild -scheme {name} -configuration Release -derivedDataPath .headway-build CODE_SIGN_IDENTITY=- build",
        artifact: Some(".headway-build/Build/Products/Release/*.app"),
        description: "macOS app from an Xcode project, ad-hoc signed. Needs Xcode.",
    },
    Preset {
        id: "tauri",
        label: "Tauri app",
        kind: AppKind::App,
        build: "pnpm install --frozen-lockfile && pnpm tauri build --bundles app",
        artifact: Some("src-tauri/target/release/bundle/macos/*.app"),
        description: "Tauri desktop app bundled as a .app.",
    },
    Preset {
        id: "tauri-local",
        label: "Tauri 2 local app",
        kind: AppKind::App,
        build: TAURI_LOCAL_BUILD,
        artifact: Some("src-tauri/target/release/bundle/macos/*.app"),
        description: "Tauri 2 local app, ad-hoc signed by default, without signed updater artifacts.",
    },
    Preset {
        id: "npm",
        label: "npm global",
        kind: AppKind::Cli,
        build: "npm ci && npm run build --if-present && npm install -g .",
        artifact: None,
        description: "Node CLI installed with npm install -g.",
    },
    Preset {
        id: "make",
        label: "Make",
        kind: AppKind::Cli,
        build: "make && make install PREFIX=\"$HOME/.local\"",
        artifact: None,
        description: "make && make install into ~/.local.",
    },
    Preset {
        id: "custom",
        label: "Custom",
        kind: AppKind::Cli,
        build: "",
        artifact: None,
        description: "Write your own build command.",
    },
];

pub fn find(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.id == id)
}

/// Fills `{bin}` and `{name}` in a path template (artifacts).
pub fn render(template: &str, bin: &str, name: &str) -> String {
    template.replace("{bin}", bin).replace("{name}", name)
}

/// Fills `{bin}` and `{name}` in a build command, shell-quoting the values: they come from
/// repository file names (`Hidden Bar.xcodeproj`, or worse, `$(…)`).
pub fn render_command(template: &str, bin: &str, name: &str) -> String {
    template
        .replace("{bin}", &crate::process::shell_quote(bin))
        .replace("{name}", &crate::process::shell_quote(name))
}

/// Picks the preset matching the files at the repository root.
pub fn detect(entries: &[ContentEntry]) -> Option<&'static Preset> {
    let has = |name: &str| entries.iter().any(|entry| entry.name == name);
    let has_suffix = |suffix: &str| entries.iter().any(|entry| entry.name.ends_with(suffix));
    let id = if has("src-tauri") {
        "tauri"
    } else if has("Cargo.toml") {
        "cargo"
    } else if has("go.mod") {
        "go"
    } else if has("Package.swift") {
        "swiftpm"
    } else if has_suffix(".xcodeproj") || has_suffix(".xcworkspace") {
        "xcode"
    } else if has("package.json") {
        "npm"
    } else if has("Makefile") || has("makefile") || has("GNUmakefile") {
        "make"
    } else {
        return None;
    };
    find(id)
}

/// Binary names declared in a `Cargo.toml`: `[[bin]]` names, else the package name.
pub fn cargo_binaries(manifest: &str) -> Vec<String> {
    let Ok(value) = manifest.parse::<toml::Table>() else {
        return Vec::new();
    };
    let bins: Vec<String> = value
        .get("bin")
        .and_then(toml::Value::as_array)
        .map(|bins| {
            bins.iter()
                .filter_map(|bin| bin.get("name").and_then(toml::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if !bins.is_empty() {
        return bins;
    }
    value
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .map(|name| vec![name.to_owned()])
        .unwrap_or_default()
}

/// Whether a root manifest explicitly includes a package in its workspace.
pub fn cargo_workspace_contains(manifest: &str, member: &str) -> bool {
    let Ok(value) = manifest.parse::<toml::Table>() else {
        return false;
    };
    let Some(workspace) = value.get("workspace").and_then(toml::Value::as_table) else {
        return false;
    };
    let contains = |key: &str| {
        workspace
            .get(key)
            .and_then(toml::Value::as_array)
            .is_some_and(|paths| {
                paths.iter().filter_map(toml::Value::as_str).any(|path| {
                    let clean = |value: &str| {
                        std::path::Path::new(value)
                            .components()
                            .filter(|part| *part != std::path::Component::CurDir)
                            .map(|part| part.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/")
                    };
                    crate::asset::glob_regex(&clean(path))
                        .is_ok_and(|pattern| pattern.is_match(&clean(member)))
                })
            })
    };
    contains("members") && !contains("exclude")
}

#[cfg(test)]
#[path = "presets_tests.rs"]
mod tests;
