use super::*;

fn base() -> TrackedApp {
    TrackedApp {
        id: "rg".into(),
        name: "ripgrep".into(),

        kind: AppKind::Cli,
        auto_update: false,
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
