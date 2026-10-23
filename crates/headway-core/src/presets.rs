//! Build presets for source installs, and detecting which one a repository needs.

use serde::Serialize;

use crate::config::AppKind;
use crate::github::ContentEntry;

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

/// Fills `{bin}` and `{name}`.
pub fn render(template: &str, bin: &str, name: &str) -> String {
    template.replace("{bin}", bin).replace("{name}", name)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(names: &[&str]) -> Vec<ContentEntry> {
        names
            .iter()
            .map(|name| ContentEntry {
                name: (*name).to_owned(),
                kind: "file".to_owned(),
            })
            .collect()
    }

    #[test]
    fn detects_build_systems() {
        let id = |names: &[&str]| detect(&entries(names)).map(|preset| preset.id);
        assert_eq!(id(&["Cargo.toml", "src"]), Some("cargo"));
        assert_eq!(
            id(&["Cargo.toml", "src-tauri", "package.json"]),
            Some("tauri")
        );
        assert_eq!(id(&["go.mod", "Makefile"]), Some("go"));
        assert_eq!(id(&["Package.swift"]), Some("swiftpm"));
        assert_eq!(id(&["Foo.xcodeproj", "README.md"]), Some("xcode"));
        assert_eq!(id(&["package.json"]), Some("npm"));
        assert_eq!(id(&["Makefile"]), Some("make"));
        assert_eq!(id(&["README.md"]), None);
    }

    #[test]
    fn renders_placeholders() {
        let cargo = find("cargo").unwrap();
        assert_eq!(
            render(cargo.artifact.unwrap(), "rg", "ripgrep"),
            "target/release/rg"
        );
        let xcode = find("xcode").unwrap();
        assert!(render(xcode.build, "x", "Ice").contains("-scheme Ice"));
        assert!(
            PRESETS
                .iter()
                .all(|preset| preset.id == "custom" || !preset.build.is_empty())
        );
    }

    #[test]
    fn reads_cargo_binaries() {
        let with_bins = "[package]\nname = \"ripgrep\"\n\n[[bin]]\nname = \"rg\"\npath = \"crates/core/main.rs\"\n";
        assert_eq!(cargo_binaries(with_bins), ["rg"]);
        assert_eq!(cargo_binaries("[package]\nname = \"bat\"\n"), ["bat"]);
        assert!(cargo_binaries("[workspace]\nmembers = [\"a\"]\n").is_empty());
        assert!(cargo_binaries("not toml [").is_empty());
    }
}
