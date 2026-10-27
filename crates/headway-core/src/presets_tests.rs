use super::*;

#[test]
fn tauri_local_preset_uses_shared_signing_and_config_flags() {
    // Given the explicit local Tauri preset.
    let preset = find("tauri-local").unwrap();
    // When reading its build command.
    let build = preset.build;
    // Then its literal command stays aligned with the generated local build flags.
    assert_eq!(
        build,
        format!(
            "pnpm install --frozen-lockfile && {TAURI_LOCAL_SIGNING} pnpm tauri build --bundles app {TAURI_LOCAL_CONFIG}"
        )
    );
}

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

#[test]
fn local_signing_defaults_only_when_identity_is_empty() {
    // Given the signing fragment shared by detection and the static preset.
    // Then an explicit identity bypasses both assignment and credential removal.
    assert_eq!(
        TAURI_LOCAL_SIGNING,
        "if [ -z \"${APPLE_SIGNING_IDENTITY:-}\" ]; then export APPLE_SIGNING_IDENTITY=-; unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_KEY_PATH APPLE_API_ISSUER; fi &&"
    );
    let build = find("tauri-local").unwrap().build;
    assert!(build.starts_with("pnpm install --frozen-lockfile && if "));
    assert!(build.contains("; fi && pnpm tauri build"));
}

#[test]
fn local_signing_shell_preserves_install_and_explicit_identity() {
    // Given dummy credentials and a package manager that only reports its environment.
    let credentials = [
        "APPLE_CERTIFICATE",
        "APPLE_CERTIFICATE_PASSWORD",
        "APPLE_ID",
        "APPLE_PASSWORD",
        "APPLE_TEAM_ID",
        "APPLE_API_KEY",
        "APPLE_API_KEY_PATH",
        "APPLE_API_ISSUER",
    ];
    for identity in [None, Some(""), Some("Developer ID Application: Example")] {
        let command = format!(
            "pnpm() {{ printf '%s:%s' \"$1\" \"${{APPLE_SIGNING_IDENTITY-unset}}\"; \
             for key in {}; do eval 'printf \":%s\" \"${{'\"$key\"'-unset}}\"'; done; printf '\\n'; }}; {} && pnpm later",
            credentials.join(" "),
            find("tauri-local").unwrap().build
        );
        // When running the generated command under its actual shell, without invoking Tauri.
        let mut shell = std::process::Command::new("/bin/sh");
        shell.env_clear().args(["-c", &command]);
        for key in credentials {
            shell.env(key, "dummy");
        }
        if let Some(identity) = identity {
            shell.env("APPLE_SIGNING_IDENTITY", identity);
        }
        let output = shell.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Then install retains credentials, and later steps inherit only the conditional defaults.
        let initial = identity.unwrap_or("unset");
        let explicit = identity.filter(|value| !value.is_empty());
        let final_identity = explicit.unwrap_or("-");
        let initial_credentials = ":dummy".repeat(credentials.len());
        let final_credentials = if explicit.is_some() {
            &initial_credentials
        } else {
            &":unset".repeat(credentials.len())
        };
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "install:{initial}{initial_credentials}\ntauri:{final_identity}{final_credentials}\nlater:{final_identity}{final_credentials}\n"
            )
        );
    }
}
