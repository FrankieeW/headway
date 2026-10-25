use super::*;

fn assets(names: &[&str]) -> Vec<Asset> {
    names
        .iter()
        .zip(1..)
        .map(|(name, id)| Asset {
            id,
            name: (*name).to_owned(),
            size: 1,
            browser_download_url: None,
            digest: None,
        })
        .collect()
}

fn pick(names: &[&str], kind: AppKind, arch: Arch) -> Option<String> {
    let assets = assets(names);
    select(&assets, kind, None, arch)
        .ok()
        .map(|asset| asset.name.clone())
}

fn pick_pattern(names: &[&str], pattern: &str, kind: AppKind, arch: Arch) -> Result<String> {
    let assets = assets(names);
    select(&assets, kind, Some(pattern), arch).map(|asset| asset.name.clone())
}

const RIPGREP: [&str; 8] = [
    "ripgrep-14.1.1-aarch64-apple-darwin.tar.gz",
    "ripgrep-14.1.1-aarch64-apple-darwin.tar.gz.sha256",
    "ripgrep-14.1.1-x86_64-apple-darwin.tar.gz",
    "ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz",
    "ripgrep-14.1.1-x86_64-pc-windows-msvc.zip",
    "ripgrep_14.1.1-1_amd64.deb",
    "ripgrep-14.1.1-aarch64-unknown-linux-gnu.tar.gz",
    "ripgrep-14.1.1-i686-pc-windows-msvc.zip",
];

#[test]
fn picks_the_matching_cpu_for_cli_tools() {
    assert_eq!(
        pick(&RIPGREP, AppKind::Cli, Arch::Arm64).unwrap(),
        "ripgrep-14.1.1-aarch64-apple-darwin.tar.gz"
    );
    assert_eq!(
        pick(&RIPGREP, AppKind::Cli, Arch::X86_64).unwrap(),
        "ripgrep-14.1.1-x86_64-apple-darwin.tar.gz"
    );
}

#[test]
fn falls_back_to_rosetta_but_never_the_other_way() {
    let intel_only = [
        "tool-x86_64-apple-darwin.tar.gz",
        "tool-x86_64-linux.tar.gz",
    ];
    assert_eq!(
        pick(&intel_only, AppKind::Cli, Arch::Arm64).unwrap(),
        "tool-x86_64-apple-darwin.tar.gz"
    );
    let arm_only = ["tool-aarch64-apple-darwin.tar.gz"];
    assert!(pick(&arm_only, AppKind::Cli, Arch::X86_64).is_none());
}

#[test]
fn prefers_dmg_for_apps_and_skips_pkg() {
    let names = [
        "Zed-aarch64.dmg",
        "Zed-x86_64.dmg",
        "Zed.pkg",
        "zed-linux-x86_64.tar.gz",
        "zed-remote-server-macos-aarch64.gz",
    ];
    assert_eq!(
        pick(&names, AppKind::App, Arch::Arm64).unwrap(),
        "Zed-aarch64.dmg"
    );
    assert!(pick(&["Tool.pkg"], AppKind::App, Arch::Arm64).is_none());
}

#[test]
fn universal_and_unmarked_assets() {
    assert_eq!(
        pick(
            &["App_universal.dmg", "App_x64.dmg"],
            AppKind::App,
            Arch::Arm64
        )
        .unwrap(),
        "App_universal.dmg"
    );
    // No OS markers anywhere: a lone archive is acceptable.
    assert_eq!(
        pick(
            &["tool.tar.gz", "SHA256SUMS.txt"],
            AppKind::Cli,
            Arch::Arm64
        )
        .unwrap(),
        "tool.tar.gz"
    );
    // Only other-OS assets: nothing to install.
    assert!(
        pick(
            &["tool-linux.tar.gz", "tool-windows.zip"],
            AppKind::Cli,
            Arch::Arm64
        )
        .is_none()
    );
}

#[test]
fn bare_binaries_are_cli_candidates() {
    assert_eq!(
        pick(
            &["jq-macos-arm64", "jq-linux-amd64", "jq-windows-amd64.exe"],
            AppKind::Cli,
            Arch::Arm64
        )
        .unwrap(),
        "jq-macos-arm64"
    );
}

#[test]
fn patterns_are_case_insensitive_globs() {
    let list = assets(&RIPGREP);
    let chosen = select(
        &list,
        AppKind::Cli,
        Some("*X86_64-APPLE-darwin.tar.gz"),
        Arch::Arm64,
    )
    .unwrap();
    assert_eq!(chosen.name, "ripgrep-14.1.1-x86_64-apple-darwin.tar.gz");
    let err = select(&list, AppKind::Cli, Some("*.dmg"), Arch::Arm64).unwrap_err();
    assert!(matches!(&err, Error::NoAsset(msg) if msg.contains("available: ripgrep-14.1.1")));
    assert!(glob_regex("a.b").unwrap().is_match("A.B"));
    assert!(!glob_regex("a.b").unwrap().is_match("axb"));
}

#[test]
fn pattern_selection_rejects_garbage_but_keeps_raw_binaries() {
    let garbage = ["tool.tar.gz.sha256", "tool-linux-x86_64.tar.gz", "tool.pkg"];
    let err = pick_pattern(&garbage, "*", AppKind::Cli, Arch::Arm64).unwrap_err();
    assert!(
        matches!(&err, Error::NoAsset(msg) if msg.contains("can be installed")),
        "{err:?}"
    );
    // An explicit glob for a raw, extension-less binary still works.
    assert_eq!(
        pick_pattern(
            &["tool-macos-arm64", "tool-macos-x86_64"],
            "tool-macos-arm64",
            AppKind::Cli,
            Arch::Arm64
        )
        .unwrap(),
        "tool-macos-arm64"
    );
}

#[test]
fn unsafe_asset_names_are_never_selected() {
    let names = [
        "../../escape",
        "a/b",
        "..",
        "bad\nname.tar.gz",
        "good-macos-arm64.tar.gz",
    ];
    assert_eq!(
        pick(&names, AppKind::Cli, Arch::Arm64).unwrap(),
        "good-macos-arm64.tar.gz"
    );
    let err = pick_pattern(&["..", "../x"], "*", AppKind::Cli, Arch::Arm64).unwrap_err();
    assert!(matches!(&err, Error::NoAsset(_)));
}

#[test]
fn safe_file_names_are_single_components_without_controls() {
    for name in ["tool", "Tool-1.2.3_macos.zip", "Hidden Bar.app.zip"] {
        assert!(safe_file_name(name), "{name}");
    }
    for name in ["", ".", "..", "a/b", "a\\b", "bad\nname", "bad\u{1b}name"] {
        assert!(!safe_file_name(name), "{name:?}");
    }
    assert!(!safe_file_name(&"x".repeat(256)));
}

#[test]
fn display_names_are_bounded_and_escape_controls() {
    assert_eq!(display_name("ok"), "ok");
    assert_eq!(display_name("a\u{1b}[31m"), "a?[31m");
    let long = display_name(&"y".repeat(500));
    assert!(long.ends_with('…'));
    assert!(long.chars().count() <= 101);
}

#[test]
fn patterns_are_validated_before_use() {
    assert!(validate_pattern("*macos*.zip").is_ok());
    assert!(validate_pattern("").is_err());
    assert!(validate_pattern("bad\npattern").is_err());
    assert!(validate_pattern(&"z".repeat(257)).is_err());
}

#[test]
fn parses_the_apple_silicon_probe() {
    assert!(parse_apple_silicon(b"1\n"));
    assert!(!parse_apple_silicon(b"0\n"));
    assert!(!parse_apple_silicon(b""));
    assert!(!parse_apple_silicon(&[0xff]));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn host_arch_is_native_on_apple_silicon() {
    assert_eq!(Arch::host(), Arch::Arm64);
}
