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
fn build_commands_quote_substituted_names() {
    let xcode = find("xcode").unwrap();
    let command = render_command(xcode.build, "x", "Hidden Bar");
    assert!(command.contains("-scheme 'Hidden Bar' "));
    let hostile = render_command(xcode.build, "x", "$(touch pwned)");
    assert!(hostile.contains("-scheme '$(touch pwned)' "));
    assert_eq!(render_command("make {bin}", "rg", "x"), "make rg");
}

#[test]
fn reads_cargo_binaries() {
    let with_bins =
        "[package]\nname = \"ripgrep\"\n\n[[bin]]\nname = \"rg\"\npath = \"crates/core/main.rs\"\n";
    assert_eq!(cargo_binaries(with_bins), ["rg"]);
    assert_eq!(cargo_binaries("[package]\nname = \"bat\"\n"), ["bat"]);
    assert!(cargo_binaries("[workspace]\nmembers = [\"a\"]\n").is_empty());
    assert!(cargo_binaries("not toml [").is_empty());
}

#[test]
fn cargo_workspace_membership_requires_positive_manifest_evidence() {
    for manifest in [
        "[workspace]\nmembers = ['crates/core', 'src-tauri']",
        "[workspace]\nmembers = ['./src-tauri/']",
    ] {
        assert!(cargo_workspace_contains(manifest, "src-tauri"));
    }
    for manifest in [
        "[package]\nname = 'standalone'",
        "[workspace]\nmembers = ['crates/core']",
        "[workspace]\nmembers = ['src-tauri']\nexclude = ['src-tauri']",
        "[workspace]\nmembers = 'src-tauri'",
        "not toml [",
    ] {
        assert!(!cargo_workspace_contains(manifest, "src-tauri"));
    }
}

#[test]
fn workspace_member_and_exclude_globs_are_respected() {
    assert!(cargo_workspace_contains(
        "[workspace]\nmembers = ['crates/*']",
        "crates/desktop"
    ));
    assert!(!cargo_workspace_contains(
        "[workspace]\nmembers = ['crates/*']\nexclude = ['crates/desk*']",
        "crates/desktop"
    ));
    assert!(cargo_workspace_contains(
        "[workspace]\nmembers = ['./src-*']",
        "src-tauri"
    ));
}
