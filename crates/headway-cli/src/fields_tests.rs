use super::*;

fn base() -> TrackedApp {
    TrackedApp {
        id: "rg".into(),
        name: "ripgrep".into(),

        kind: AppKind::Cli,
        auto_update: false,
        update_policy: headway_core::UpdatePolicy::default(),
        source: Source::Github(GithubSource {
            options: Box::default(),
            repo: "BurntSushi/ripgrep".into(),
            git_url: None,
            track: Track::Commit { branch: None },
            install: Install::Source(SourceInstall {
                build: "cargo build --release --locked".into(),
                artifact: Some("target/release/rg".into()),
                ..SourceInstall::default()
            }),
        }),
    }
}

#[test]
fn no_flags_keep_the_base() {
    assert_eq!(apply(base(), &AppFields::default()).unwrap(), base());
}

#[test]
fn tool_versions_can_be_set_and_cleared_without_changing_the_build() {
    let app = apply(
        base(),
        &AppFields {
            tools: Some("zig@0.16.0, node@24.14.0".into()),
            ..AppFields::default()
        },
    )
    .unwrap();
    assert!(
        matches!(&app.github().unwrap().install, Install::Source(source) if source.tools == ["zig@0.16.0", "node@24.14.0"] && source.build == "cargo build --release --locked")
    );
    let cleared = apply(
        app,
        &AppFields {
            tools: Some(String::new()),
            ..AppFields::default()
        },
    )
    .unwrap();
    assert!(
        matches!(&cleared.github().unwrap().install, Install::Source(source) if source.tools.is_empty())
    );
}

#[test]
fn source_presets_select_the_payload_kind_unless_explicitly_overridden() {
    // Given an app suggestion, when choosing the standalone CLI recipe.
    let desktop = TrackedApp {
        kind: AppKind::App,
        ..base()
    };
    let fields = AppFields {
        preset: Some("headway-cli".into()),
        ..AppFields::default()
    };
    let cli = apply(desktop.clone(), &fields).unwrap();
    // Then its artifact goes to bin_dir, and an explicit kind still takes precedence.
    assert_eq!(cli.kind, AppKind::Cli);
    let explicit = apply(
        desktop,
        &AppFields {
            kind: Some(KindArg::App),
            ..fields
        },
    )
    .unwrap();
    assert_eq!(explicit.kind, AppKind::App);
}

#[test]
fn switching_to_release_assets() {
    let fields = AppFields {
        track: Some(TrackArg::Release),
        method: Some(MethodArg::Asset),
        pattern: Some("*aarch64-apple-darwin.tar.gz".into()),
        binary: Some("rg".into()),
        prerelease: Some(true),
        ..AppFields::default()
    };
    let app = apply(base(), &fields).unwrap();
    assert_eq!(
        app.github().unwrap().track,
        Track::Release {
            prerelease: true,
            tag_filter: None
        }
    );
    assert_eq!(
        app.github().unwrap().install,
        Install::Asset(AssetInstall {
            verify_command: None,
            pattern: Some("*aarch64-apple-darwin.tar.gz".into()),
            binary: Some("rg".into()),
            dest: None,
        })
    );
    assert_eq!(
        app.github()
            .unwrap()
            .options
            .source_build
            .as_ref()
            .map(|source| source.build.as_str()),
        Some("cargo build --release --locked")
    );
    app.validate().unwrap();
}

#[test]
fn source_and_asset_recipes_can_be_saved_together() {
    // Given a source build, when a release pattern is added without changing the default.
    let app = apply(
        base(),
        &AppFields {
            pattern: Some("*.tar.gz".into()),
            binary: Some("rg".into()),
            ..AppFields::default()
        },
    )
    .unwrap();
    let github = app.github().unwrap();
    // Then ordinary updates still build, and the download recipe is kept beside it.
    assert!(
        matches!(&github.install, Install::Source(source) if source.build.contains("cargo build"))
    );
    let asset = github.options.release_asset.as_ref().unwrap();
    assert_eq!(asset.pattern.as_deref(), Some("*.tar.gz"));
    assert_eq!(asset.binary.as_deref(), Some("rg"));
    app.validate().unwrap();
}

#[test]
fn an_asset_default_can_keep_a_source_build() {
    let app = apply(
        base(),
        &AppFields {
            method: Some(MethodArg::Asset),
            pattern: Some("*.zip".into()),
            build: Some("make".into()),
            ..AppFields::default()
        },
    )
    .unwrap();
    let github = app.github().unwrap();
    assert!(
        matches!(&github.install, Install::Asset(asset) if asset.pattern.as_deref() == Some("*.zip"))
    );
    assert_eq!(
        github
            .options
            .source_build
            .as_ref()
            .map(|source| source.build.as_str()),
        Some("make")
    );
    app.validate().unwrap();
}

#[test]
fn presets_fill_the_build_and_flags_override() {
    let fields = AppFields {
        preset: Some("go".into()),
        binary: Some("lazygit".into()),
        dest: Some("~/bin/lg".into()),
        ..AppFields::default()
    };
    let app = apply(base(), &fields).unwrap();
    let Install::Source(source) = &app.github().unwrap().install else {
        panic!("source")
    };
    assert_eq!(source.build, "go build -trimpath -o .headway-out/lazygit .");
    assert_eq!(source.artifact.as_deref(), Some(".headway-out/lazygit"));
    assert_eq!(source.dest.as_deref(), Some("~/bin/lg"));
}

#[test]
fn empty_strings_clear_optional_fields() {
    let fields = AppFields {
        artifact: Some(String::new()),
        branch: Some("dev".into()),
        ..AppFields::default()
    };
    let app = apply(base(), &fields).unwrap();
    assert_eq!(
        app.github().unwrap().track,
        Track::Commit {
            branch: Some("dev".into())
        }
    );
    assert!(
        matches!(&app.github().unwrap().install, Install::Source(source) if source.artifact.is_none())
    );
    let cleared = apply(
        app,
        &AppFields {
            branch: Some(String::new()),
            ..AppFields::default()
        },
    )
    .unwrap();
    assert_eq!(
        cleared.github().unwrap().track,
        Track::Commit { branch: None }
    );
}

#[test]
fn source_retention_needs_a_source_build() {
    let Source::Github(github) = base().source else {
        panic!("github");
    };
    let asset_only = TrackedApp {
        source: Source::Github(GithubSource {
            install: Install::Asset(AssetInstall {
                pattern: Some("*.tar.gz".into()),
                ..AssetInstall::default()
            }),
            ..github
        }),
        ..base()
    };
    for fields in [
        AppFields {
            keep_source: Some(false),
            ..AppFields::default()
        },
        AppFields {
            source_root: Some("~/src".into()),
            ..AppFields::default()
        },
    ] {
        let error = apply(asset_only.clone(), &fields).unwrap_err();
        assert!(
            matches!(error, Error::InvalidInput(ref message) if message.contains("a GitHub source build")),
            "{error}"
        );
    }
    let kept = apply(
        asset_only,
        &AppFields {
            method: Some(MethodArg::Asset),
            build: Some("make".into()),
            keep_source: Some(false),
            ..AppFields::default()
        },
    )
    .unwrap();
    let github = kept.github().unwrap();
    assert!(matches!(github.install, Install::Asset(_)));
    assert_eq!(
        github
            .options
            .source_build
            .as_ref()
            .map(|source| (source.build.as_str(), source.storage.keep)),
        Some(("make", false))
    );
}

#[test]
fn mismatched_flags_are_rejected() {
    let cases = [
        AppFields {
            branch: Some("x".into()),
            track: Some(TrackArg::Release),
            ..AppFields::default()
        },
        AppFields {
            prerelease: Some(true),
            ..AppFields::default()
        },
        AppFields {
            preset: Some("nope".into()),
            ..AppFields::default()
        },
    ];
    for fields in cases {
        assert!(
            matches!(apply(base(), &fields), Err(Error::InvalidInput(_))),
            "{fields:?}"
        );
    }
}

#[test]
fn policy_and_verification_flags_apply_and_clear() {
    // Given explicit policy and verification flags.
    let fields = AppFields {
        update_mode: Some(UpdateModeArg::Next),
        max_update_steps: Some(4),
        pause_on_failure: Some(false),
        verify_command: Some("tool --version".into()),
        ..AppFields::default()
    };
    // When applying the flags.
    let app = apply(base(), &fields).unwrap();
    // Then policy values and the verification command are preserved.
    assert_eq!(
        app.update_policy,
        UpdatePolicy {
            mode: AutoUpdateMode::Next,
            max_steps: 4,
            pause_on_failure: false
        }
    );
    assert!(
        matches!(&app.github().unwrap().install, Install::Source(source) if source.verify_command.as_deref() == Some("tool --version"))
    );
    let cleared = apply(
        app,
        &AppFields {
            verify_command: Some(String::new()),
            ..AppFields::default()
        },
    )
    .unwrap();
    assert!(
        matches!(&cleared.github().unwrap().install, Install::Source(source) if source.verify_command.is_none())
    );
}

#[test]
fn verification_flag_supports_assets_and_rejects_non_github_sources() {
    // Given release-asset and npm sources.
    let fields = AppFields {
        track: Some(TrackArg::Release),
        method: Some(MethodArg::Asset),
        verify_command: Some("tool --version".into()),
        ..AppFields::default()
    };
    // When applying a verification command to each source.
    let asset = apply(base(), &fields).unwrap();
    let npm = TrackedApp {
        source: Source::Npm(headway_core::NpmSource {
            package: "tool".into(),
            dist_tag: "latest".into(),
        }),
        ..base()
    };
    let error = apply(
        npm,
        &AppFields {
            verify_command: Some("true".into()),
            ..AppFields::default()
        },
    )
    .unwrap_err();
    // Then both install methods support verification, and npm rejects the flag.
    assert!(
        matches!(&asset.github().unwrap().install, Install::Asset(spec) if spec.verify_command.as_deref() == Some("tool --version"))
    );
    assert!(
        error
            .to_string()
            .contains("--verify-command only applies to GitHub")
    );
}

#[test]
fn policy_flags_parse_and_reject_out_of_range_steps() {
    use crate::args::Cli;
    use clap::Parser as _;
    // Given the documented command-line policy options.
    let args = [
        "headway",
        "edit",
        "tool",
        "--update-mode",
        "next",
        "--max-update-steps",
        "20",
        "--pause-on-failure",
        "false",
        "--verify-command",
        "",
    ];
    // When parsing valid flags and out-of-range step counts.
    assert!(Cli::try_parse_from(args).is_ok());
    // Then range errors are rejected by clap before persistence.
    for value in ["0", "21", "256"] {
        assert!(
            Cli::try_parse_from(["headway", "edit", "tool", "--max-update-steps", value]).is_err()
        );
    }
}

#[path = "fields_recipe_tests.rs"]
mod recipes;
