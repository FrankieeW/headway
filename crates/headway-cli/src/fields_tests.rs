use super::*;

fn base() -> TrackedApp {
    TrackedApp {
        id: "rg".into(),
        name: "ripgrep".into(),

        kind: AppKind::Cli,
        auto_update: false,
        update_policy: headway_core::UpdatePolicy::default(),
        source: Source::Github(GithubSource {
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
    app.validate().unwrap();
}

#[test]
fn leaving_release_mode_switches_assets_back_to_source() {
    let release = apply(
        base(),
        &AppFields {
            track: Some(TrackArg::Release),
            method: Some(MethodArg::Asset),
            ..AppFields::default()
        },
    )
    .unwrap();
    let tags = apply(
        release,
        &AppFields {
            track: Some(TrackArg::Tag),
            build: Some("make".into()),
            ..AppFields::default()
        },
    )
    .unwrap();
    assert!(
        matches!(&tags.github().unwrap().install, Install::Source(source) if source.build == "make")
    );
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
            pattern: Some("*".into()),
            ..AppFields::default()
        },
        AppFields {
            track: Some(TrackArg::Release),
            method: Some(MethodArg::Asset),
            build: Some("make".into()),
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
