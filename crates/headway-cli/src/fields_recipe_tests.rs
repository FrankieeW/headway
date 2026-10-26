use super::*;

fn asset_default() -> TrackedApp {
    apply(
        base(),
        &AppFields {
            method: Some(MethodArg::Asset),
            pattern: Some("*.zip".into()),
            ..AppFields::default()
        },
    )
    .unwrap()
}

/// A download-only app: no source build is saved beside it.
fn asset_only(kind: AppKind) -> TrackedApp {
    let Source::Github(github) = base().source else {
        panic!("github");
    };
    TrackedApp {
        kind,
        source: Source::Github(GithubSource {
            install: Install::Asset(AssetInstall {
                pattern: Some("*.dmg".into()),
                ..AssetInstall::default()
            }),
            ..github
        }),
        ..base()
    }
}

#[test]
fn a_preset_for_the_alternate_build_keeps_the_payload_kind() {
    // Given a downloaded .app, when a CLI preset fills only the alternate build.
    let fields = AppFields {
        preset: Some("cargo".into()),
        ..AppFields::default()
    };
    let edited = apply(asset_only(AppKind::App), &fields).unwrap();
    // Then the download still installs an app bundle.
    assert_eq!(edited.kind, AppKind::App);
    assert_eq!(
        edited.github().unwrap().install,
        asset_only(AppKind::App).github().unwrap().install
    );
    // When the preset build becomes the default, its payload kind follows.
    let built = apply(
        asset_only(AppKind::App),
        &AppFields {
            method: Some(MethodArg::Source),
            ..fields
        },
    )
    .unwrap();
    assert_eq!(built.kind, AppKind::Cli);
}

#[test]
fn source_paths_without_a_build_are_rejected_instead_of_dropped() {
    for fields in [
        AppFields {
            workdir: Some("cli".into()),
            ..AppFields::default()
        },
        AppFields {
            artifact: Some("out/tool".into()),
            ..AppFields::default()
        },
    ] {
        let error = apply(asset_only(AppKind::Cli), &fields).unwrap_err();
        assert!(
            matches!(&error, Error::InvalidInput(message) if message.contains("pass --build or --preset")),
            "{error}"
        );
    }
}

#[test]
fn editing_source_recipe_fields_keeps_the_asset_default() {
    let cases = [
        AppFields {
            build: Some("make release".into()),
            ..AppFields::default()
        },
        AppFields {
            workdir: Some("cli".into()),
            ..AppFields::default()
        },
        AppFields {
            artifact: Some("out/tool".into()),
            ..AppFields::default()
        },
        AppFields {
            preset: Some("go".into()),
            ..AppFields::default()
        },
    ];
    for fields in cases {
        let before = asset_default();
        let edited = apply(before.clone(), &fields).unwrap();
        let github = edited.github().unwrap();
        assert_eq!(github.install, before.github().unwrap().install);
        let source = github.options.source_build.as_ref().unwrap();
        if let Some(build) = fields.build {
            assert_eq!(source.build, build);
        }
        if let Some(workdir) = fields.workdir {
            assert_eq!(source.workdir.as_deref(), Some(workdir.as_str()));
        }
        if let Some(artifact) = fields.artifact {
            assert_eq!(source.artifact.as_deref(), Some(artifact.as_str()));
        }
        if fields.preset.is_some() {
            assert_eq!(source.build, "go build -trimpath -o .headway-out/ripgrep .");
        }
        edited.validate().unwrap();
    }
}

#[test]
fn clearing_the_alternate_build_removes_it_without_changing_the_default() {
    for build in ["", " \n "] {
        let before = asset_default();
        let cleared = apply(
            before.clone(),
            &AppFields {
                method: Some(MethodArg::Asset),
                build: Some(build.into()),
                ..AppFields::default()
            },
        )
        .unwrap();
        let github = cleared.github().unwrap();
        assert_eq!(github.install, before.github().unwrap().install);
        assert!(github.options.source_build.is_none());
        cleared.validate().unwrap();
    }
}

#[test]
fn explicit_method_switches_preserve_edits_and_keep_only_the_alternate() {
    let edited = apply(
        asset_default(),
        &AppFields {
            build: Some("make release".into()),
            ..AppFields::default()
        },
    )
    .unwrap();
    let source = apply(
        edited,
        &AppFields {
            method: Some(MethodArg::Source),
            track: Some(TrackArg::Tag),
            ..AppFields::default()
        },
    )
    .unwrap();
    let github = source.github().unwrap();
    assert!(matches!(&github.install, Install::Source(source) if source.build == "make release"));
    assert!(github.options.source_build.is_none());
    assert_eq!(
        github
            .options
            .release_asset
            .as_ref()
            .unwrap()
            .pattern
            .as_deref(),
        Some("*.zip")
    );
    source.validate().unwrap();
    let asset = apply(
        source,
        &AppFields {
            method: Some(MethodArg::Asset),
            pattern: Some("*.tar.gz".into()),
            ..AppFields::default()
        },
    )
    .unwrap();
    let github = asset.github().unwrap();
    assert!(
        matches!(&github.install, Install::Asset(asset) if asset.pattern.as_deref() == Some("*.tar.gz"))
    );
    assert!(github.options.release_asset.is_none());
    assert_eq!(
        github.options.source_build.as_ref().unwrap().build,
        "make release"
    );
    asset.validate().unwrap();
}
