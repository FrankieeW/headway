//! Picking the release asset that fits this Mac.
//!
//! Asset names are lowercased before any suffix test, hence the lint allowance.
#![allow(clippy::case_sensitive_file_extension_comparisons)]

use std::sync::OnceLock;

use regex::Regex;

use crate::config::AppKind;
use crate::error::{Error, Result};
use crate::github::Asset;

/// The CPU architecture release assets are selected for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm64,
    X86_64,
}

impl Arch {
    /// The Mac's real CPU architecture, not the one this binary was compiled for.
    ///
    /// Headway can itself run under Rosetta on Apple Silicon; in that case native
    /// arm64 assets are still the right choice for the host. The `x86_64` build probes
    /// the kernel's `hw.optional.arm64` flag with an empty environment and no shell —
    /// no repository or build code ever runs in that probe — and falls back to the
    /// compile-time architecture if the probe is unavailable.
    pub fn host() -> Self {
        static HOST: OnceLock<Arch> = OnceLock::new();
        *HOST.get_or_init(detect_host)
    }
}

/// Whether a GitHub asset name can be joined into a scratch directory safely.
///
/// Rejects empty names, path separators, `.`/`..`, control characters (which could
/// also corrupt log output) and over-long names. The name is still untrusted data;
/// this only guarantees it is a single ordinary file-system component.
pub fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

/// A bounded, control-character-free rendering of an untrusted asset name.
pub fn display_name(name: &str) -> String {
    const LIMIT: usize = 100;
    let mut out = String::with_capacity(name.len().min(LIMIT));
    for (index, character) in name.chars().enumerate() {
        if index >= LIMIT {
            out.push('…');
            return out;
        }
        out.push(if character.is_control() {
            '?'
        } else {
            character
        });
    }
    out
}

/// Validates a user-supplied asset glob before it is saved in the config.
pub fn validate_pattern(pattern: &str) -> Result<()> {
    if pattern.is_empty() {
        return Err(Error::InvalidInput(
            "asset pattern is empty; omit it to pick the best macOS asset".into(),
        ));
    }
    if pattern.len() > 256 {
        return Err(Error::InvalidInput(
            "asset pattern must be at most 256 bytes".into(),
        ));
    }
    if pattern.chars().any(char::is_control) {
        return Err(Error::InvalidInput(
            "asset pattern must not contain control characters".into(),
        ));
    }
    glob_regex(pattern)?;
    Ok(())
}

fn detect_host() -> Arch {
    if cfg!(target_arch = "aarch64") {
        return Arch::Arm64;
    }
    if cfg!(target_os = "macos") && running_on_apple_silicon() {
        return Arch::Arm64;
    }
    match std::env::consts::ARCH {
        "x86_64" => Arch::X86_64,
        _ => Arch::Arm64,
    }
}

fn running_on_apple_silicon() -> bool {
    let probe = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.optional.arm64"])
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    probe.is_ok_and(|output| output.status.success() && parse_apple_silicon(&output.stdout))
}

fn parse_apple_silicon(stdout: &[u8]) -> bool {
    std::str::from_utf8(stdout).is_ok_and(|text| text.trim() == "1")
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
/// Names that could not be turned into a scratch file, or that cannot be installed on
/// this machine (checksum, signature, other-OS, unsupported package), are never
/// selected — with or without a pattern.
pub fn select<'a>(
    assets: &'a [Asset],
    kind: AppKind,
    pattern: Option<&str>,
    arch: Arch,
) -> Result<&'a Asset> {
    let usable: Vec<&'a Asset> = assets
        .iter()
        .filter(|asset| safe_file_name(&asset.name))
        .collect();

    if let Some(pattern) = pattern {
        let glob = glob_regex(pattern)?;
        let matching: Vec<&'a Asset> = usable
            .iter()
            .copied()
            .filter(|asset| glob.is_match(&asset.name))
            .collect();
        if matching.is_empty() {
            return Err(Error::NoAsset(format!(
                "nothing matches `{}` ({})",
                display_name(pattern),
                available(assets.iter())
            )));
        }
        return matching
            .iter()
            .copied()
            .filter_map(|asset| score(&asset.name, kind, arch).map(|value| (value, asset)))
            .max_by_key(|(value, _)| *value)
            .map(|(_, asset)| asset)
            .ok_or_else(|| {
                Error::NoAsset(format!(
                    "nothing matching `{}` can be installed on {} ({})",
                    display_name(pattern),
                    arch_label(arch),
                    available(matching.iter().copied())
                ))
            });
    }

    let any_os_specific = assets.iter().any(|asset| {
        let name = asset.name.to_ascii_lowercase();
        MAC_MARKERS
            .iter()
            .chain(OTHER_OS_MARKERS.iter())
            .any(|marker| name.contains(marker))
    });
    usable
        .iter()
        .copied()
        .filter_map(|asset| score(&asset.name, kind, arch).map(|value| (value, asset)))
        .filter(|(value, _)| !any_os_specific || *value >= MAC_SCORE)
        .max_by_key(|(value, _)| *value)
        .map(|(_, asset)| asset)
        .ok_or_else(|| {
            Error::NoAsset(format!(
                "no macOS {} asset found ({})",
                arch_label(arch),
                available(assets.iter())
            ))
        })
}

/// Bounded, sanitized listing of asset names for an error message.
fn available<'a>(assets: impl Iterator<Item = &'a Asset>) -> String {
    let mut names: Vec<String> = assets
        .map(|asset| display_name(&asset.name))
        .collect::<Vec<_>>();
    if names.is_empty() {
        return "the release has no assets".to_owned();
    }
    let total = names.len();
    names.truncate(20);
    if total > names.len() {
        names.push(format!("… and {} more", total - names.len()));
    }
    format!("available: {}", names.join(", "))
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
#[path = "asset_tests.rs"]
mod tests;
