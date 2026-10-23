//! Picking the release asset that fits this Mac.
//!
//! Asset names are lowercased before any suffix test, hence the lint allowance.
#![allow(clippy::case_sensitive_file_extension_comparisons)]

use regex::Regex;

use crate::config::AppKind;
use crate::error::{Error, Result};
use crate::github::Asset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm64,
    X86_64,
}

impl Arch {
    pub fn host() -> Self {
        if std::env::consts::ARCH == "x86_64" {
            Self::X86_64
        } else {
            Self::Arm64
        }
    }
}

const NOT_INSTALLABLE_SUFFIXES: [&str; 19] = [
    ".sha256",
    ".sha512",
    ".sha1",
    ".md5",
    ".sig",
    ".asc",
    ".minisig",
    ".pem",
    ".crt",
    ".sbom",
    ".spdx",
    ".json",
    ".txt",
    ".md",
    ".yml",
    ".yaml",
    ".blockmap",
    ".sum",
    ".intoto.jsonl",
];
const OTHER_OS_MARKERS: [&str; 18] = [
    "linux",
    "windows",
    "win32",
    "win64",
    "-win-",
    "_win_",
    ".exe",
    ".msi",
    ".deb",
    ".rpm",
    ".apk",
    ".appimage",
    "freebsd",
    "netbsd",
    "openbsd",
    "android",
    "illumos",
    "solaris",
];
const MAC_MARKERS: [&str; 8] = [
    "darwin",
    "macos",
    "mac-",
    "-mac",
    "_mac",
    "osx",
    "apple",
    "universal",
];

/// Chooses the asset to install.
///
/// With a `pattern` (a glob, case-insensitive) only matching assets are considered;
/// otherwise assets are scored for macOS, the CPU architecture and the archive format.
pub fn select<'a>(
    assets: &'a [Asset],
    kind: AppKind,
    pattern: Option<&str>,
    arch: Arch,
) -> Result<&'a Asset> {
    let names = || {
        let list: Vec<&str> = assets.iter().map(|asset| asset.name.as_str()).collect();
        if list.is_empty() {
            "the release has no assets".to_owned()
        } else {
            format!("available: {}", list.join(", "))
        }
    };

    if let Some(pattern) = pattern {
        let glob = glob_regex(pattern)?;
        let matching: Vec<&Asset> = assets
            .iter()
            .filter(|asset| glob.is_match(&asset.name))
            .collect();
        return matching
            .iter()
            .copied()
            .max_by_key(|asset| score(&asset.name, kind, arch).unwrap_or(i32::MIN))
            .ok_or_else(|| Error::NoAsset(format!("nothing matches `{pattern}` ({})", names())));
    }

    let any_os_specific = assets.iter().any(|asset| {
        let name = asset.name.to_ascii_lowercase();
        MAC_MARKERS
            .iter()
            .chain(OTHER_OS_MARKERS.iter())
            .any(|marker| name.contains(marker))
    });
    assets
        .iter()
        .filter_map(|asset| score(&asset.name, kind, arch).map(|value| (value, asset)))
        .filter(|(value, _)| !any_os_specific || *value >= MAC_SCORE)
        .max_by_key(|(value, _)| *value)
        .map(|(_, asset)| asset)
        .ok_or_else(|| {
            Error::NoAsset(format!(
                "no macOS {} asset found ({})",
                arch_label(arch),
                names()
            ))
        })
}

const MAC_SCORE: i32 = 100;

fn arch_label(arch: Arch) -> &'static str {
    match arch {
        Arch::Arm64 => "arm64",
        Arch::X86_64 => "x86_64",
    }
}

/// `None` means the asset can't be installed on this machine at all.
fn score(name: &str, kind: AppKind, arch: Arch) -> Option<i32> {
    let name = name.to_ascii_lowercase();
    let not_installable = NOT_INSTALLABLE_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
        || name.contains("checksum")
        || name.contains("sha256sum");
    if not_installable || OTHER_OS_MARKERS.iter().any(|marker| name.contains(marker)) {
        return None;
    }

    let mac = MAC_MARKERS.iter().any(|marker| name.contains(marker))
        || name.ends_with(".dmg")
        || name.ends_with(".pkg");
    let arm = name.contains("aarch64") || name.contains("arm64");
    let intel = ["x86_64", "x86-64", "amd64", "x64", "intel"]
        .iter()
        .any(|marker| name.contains(marker));
    let universal = name.contains("universal");

    let arch_score = match arch {
        Arch::Arm64 if arm || universal => 30,
        // Rosetta runs Intel builds, but only as a last resort.
        Arch::Arm64 if intel => 5,
        Arch::X86_64 if intel || universal => 30,
        Arch::X86_64 if arm => return None,
        _ => 15,
    };
    let format_score = format_score(&name, kind)?;
    Some(if mac { MAC_SCORE } else { 0 } + arch_score + format_score)
}

fn format_score(name: &str, kind: AppKind) -> Option<i32> {
    let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| name.ends_with(suffix));
    let is_archive = ends(&[
        ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz", ".tar.zst", ".zip", ".tar",
    ]);
    // A bare binary has no extension, or only a version-like one (`tool-1.2.3-macos`).
    let extension = name.rsplit_once('.').map(|(_, ext)| ext);
    let bare = extension.is_none_or(|ext| ext.len() > 4 || !ext.chars().all(char::is_alphabetic));
    if ends(&[".pkg"]) {
        // Installer packages need root; not supported.
        return None;
    }
    let score = match kind {
        AppKind::Cli if ends(&[".tar.gz", ".tgz"]) => 20,
        AppKind::Cli if ends(&[".tar.xz", ".txz"]) => 18,
        AppKind::Cli if ends(&[".zip"]) => 15,
        AppKind::Cli if is_archive => 14,
        AppKind::Cli if ends(&[".dmg"]) => 2,
        AppKind::Cli if bare => 10,
        AppKind::App if ends(&[".dmg"]) => 20,
        AppKind::App if ends(&[".zip"]) => 18,
        AppKind::App if ends(&[".app.tar.gz"]) => 17,
        AppKind::App if is_archive => 12,
        AppKind::Cli | AppKind::App => return None,
    };
    Some(score)
}

/// Converts a glob (`*`, `?`) into an anchored, case-insensitive regex.
pub fn glob_regex(pattern: &str) -> Result<Regex> {
    let mut source = String::from("(?i)^");
    for c in pattern.chars() {
        match c {
            '*' => source.push_str(".*"),
            '?' => source.push('.'),
            c => source.push_str(&regex::escape(&c.to_string())),
        }
    }
    source.push('$');
    Regex::new(&source).map_err(|err| Error::InvalidInput(format!("pattern `{pattern}`: {err}")))
}

#[cfg(test)]
mod tests {
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
}
