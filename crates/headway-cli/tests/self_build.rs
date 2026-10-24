//! Exercise the real build recipe with offline compiler/package-manager fixtures.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::fs;
#[expect(
    dead_code,
    reason = "Shared fixture also serves the self_follow integration target."
)]
mod self_build_support;
use self_build_support::{Repo, success};

const HOST: &str = "aarch64-apple-darwin";
const CROSS: &str = "x86_64-apple-darwin";
const CLI: &str = ".headway-out/headway";

#[test]
fn successive_commits_and_repeat_build_retain_cache_with_exact_stamps() {
    // Given a committed checkout and an existing compiler cache.
    let repo = Repo::new();
    let first = repo.git(&["rev-parse", "HEAD"]);
    let sentinel = repo.root.join("target/cache-sentinel");
    fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    fs::write(&sentinel, "retained").unwrap();
    // When the same helper builds two commits, then repeats the second.
    success(repo.build(&["cli"], &[]));
    let original = repo.stamp(CLI);
    assert_eq!(original.commit, first);
    assert!(!original.dirty);
    assert_eq!(original.target, HOST);
    let second = repo.commit("second");
    assert_ne!(first, second);
    for _ in 0..2 {
        let output = success(repo.build(&["cli"], &[]));
        // Then the staged executable reports the exact current commit.
        let stamp = repo.stamp(CLI);
        assert_eq!(stamp.commit, second);
        assert!(!stamp.dirty);
        assert_eq!(stamp.target, HOST);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            repo.root.join(CLI).to_str().unwrap()
        );
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "retained");
    }
    assert_eq!(
        repo.tools()
            .lines()
            .filter(|line| line.starts_with("cargo "))
            .count(),
        3
    );
}

#[test]
fn selected_target_and_spaced_cache_override_stale_artifacts() {
    let repo = Repo::new();
    for relative in [
        "target/release",
        "target/aarch64-apple-darwin/release",
        "cache with spaces/aarch64-apple-darwin/release",
    ] {
        let dir = repo.root.join(relative);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("headway"), "stale").unwrap();
    }
    fs::write(repo.root.join("revision"), "uncommitted").unwrap();
    success(repo.build(
        &["cli", "--target", CROSS],
        &[
            ("CARGO_TARGET_DIR", "cache with spaces"),
            ("CARGO_BUILD_TARGET", HOST),
        ],
    ));
    let stamp = repo.stamp(CLI);
    assert_eq!(stamp.commit, repo.git(&["rev-parse", "HEAD"]));
    assert!(stamp.dirty);
    assert_eq!(stamp.target, CROSS);
    assert_eq!(
        fs::read(repo.root.join(CLI)).unwrap(),
        fs::read(
            repo.root
                .join(format!("cache with spaces/{CROSS}/release/headway"))
        )
        .unwrap()
    );
}

#[test]
fn failed_build_preserves_published_cli() {
    let repo = Repo::new();
    success(repo.build(&["cli"], &[]));
    let old = fs::read(repo.root.join(CLI)).unwrap();
    repo.commit("second");
    let output = repo.build(&["cli"], &[("FAIL_BUILD", "1")]);
    assert_eq!(output.status.code(), Some(42));
    assert_eq!(fs::read(repo.root.join(CLI)).unwrap(), old);
    assert!(output.stdout.is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn app_bundles_selected_cli_without_dmg_and_accepts_changed_bytes() {
    let repo = Repo::new();
    for target in [HOST, CROSS] {
        success(repo.build(
            &["app"],
            &[
                ("CARGO_TARGET_DIR", "cache with spaces"),
                ("CARGO_BUILD_TARGET", target),
            ],
        ));
        let bundled = ".headway-out/Headway.app/Contents/MacOS/headway";
        let built = format!("cache with spaces/{target}/release/headway");
        assert_eq!(repo.stamp(bundled), repo.stamp(&built));
        assert_eq!(repo.stamp(bundled).target, target);
        assert_ne!(
            fs::read(repo.root.join(bundled)).unwrap(),
            fs::read(repo.root.join(&built)).unwrap()
        );
        assert!(
            !repo
                .root
                .join(format!("cache with spaces/{target}/release/bundle/dmg"))
                .exists()
        );
    }
    assert_eq!(
        repo.tools()
            .lines()
            .filter(|line| *line == "pnpm install --frozen-lockfile")
            .count(),
        2
    );
    assert_eq!(
        repo.tools()
            .lines()
            .filter(|line| line.starts_with("codesign --verify "))
            .count(),
        2
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_app_metadata_mismatch_preserves_published_bundle() {
    let repo = Repo::new();
    success(repo.build(&["app"], &[]));
    let bundled = ".headway-out/Headway.app/Contents/MacOS/headway";
    let old = fs::read(repo.root.join(bundled)).unwrap();
    repo.commit("second");
    let output = repo.build(&["app"], &[("MISMATCH", "1")]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(repo.root.join(bundled)).unwrap(), old);
}

#[cfg(target_os = "macos")]
#[test]
fn failed_signature_preserves_published_bundle() {
    // Given a published bundle with a resource absent from the next build.
    let repo = Repo::new();
    success(repo.build(&["app"], &[]));
    let bundle = repo.root.join(".headway-out/Headway.app/Contents");
    fs::create_dir(bundle.join("Resources")).unwrap();
    fs::write(bundle.join("Resources/previous"), "keep resource").unwrap();
    let cli = fs::read(bundle.join("MacOS/headway")).unwrap();
    let desktop = fs::read(bundle.join("MacOS/headway-desktop")).unwrap();
    repo.commit("second");

    // When strict verification rejects the newly staged bundle.
    let output = repo.build(&["app"], &[("FAIL_SIGNATURE", "1")]);

    // Then the failure propagates and every previous bundle file survives.
    assert_eq!(output.status.code(), Some(43));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(bundle.join("MacOS/headway")).unwrap(), cli);
    assert_eq!(
        fs::read(bundle.join("MacOS/headway-desktop")).unwrap(),
        desktop
    );
    assert_eq!(
        fs::read_to_string(bundle.join("Resources/previous")).unwrap(),
        "keep resource"
    );
    assert_eq!(
        repo.tools()
            .lines()
            .filter(|line| line.starts_with("codesign --verify "))
            .count(),
        2
    );
    println!("signature failure: exit=43; previous CLI, desktop, resource preserved");
}

#[cfg(target_os = "macos")]
#[test]
fn app_linker_environment_is_scoped_before_the_bundled_cli_build() {
    // Given an inherited Homebrew-style library search path.
    let repo = Repo::new();
    // When the app recipe builds its CLI, it has already selected the tracked SDK path.
    success(repo.build(
        &["app"],
        &[
            ("LIBRARY_PATH", "/fixture/inherited"),
            ("CHECK_APP_ENV", "1"),
        ],
    ));
    // Then the separate CLI-only recipe still honors the original environment contract.
    success(repo.build(
        &["cli"],
        &[
            ("LIBRARY_PATH", "/fixture/inherited"),
            ("CHECK_CLI_ENV", "1"),
        ],
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn nonportable_or_hardened_adhoc_artifacts_never_replace_the_published_bundle() {
    // Given an existing successful app build.
    let repo = Repo::new();
    success(repo.build(&["app"], &[]));
    let path = ".headway-out/Headway.app/Contents/MacOS/headway";
    let old = fs::read(repo.root.join(path)).unwrap();
    // When either release boundary rejects a subsequent build.
    for flag in ["FOREIGN_DEPENDENCY", "WRONG_RUNTIME"] {
        let result = repo.build(&["app"], &[(flag, "1")]);
        // Then publication stops and the previous app survives unchanged.
        assert!(!result.status.success());
        assert_eq!(fs::read(repo.root.join(path)).unwrap(), old);
    }
}
