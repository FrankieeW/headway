use super::*;
use crate::source::{BrewSource, NpmSource, ScriptSource};
use crate::testing::{cli_app, with_github};

fn zed() -> TrackedApp {
    TrackedApp {
        id: "zed".into(),
        name: "Zed".into(),
        kind: AppKind::App,
        auto_update: true,
        update_policy: crate::UpdatePolicy::default(),
        source: Source::Github(GithubSource {
            repo: "zed-industries/zed".into(),
            git_url: Some("git@github.com:zed-industries/zed.git".into()),
            track: Track::Release {
                prerelease: true,
                tag_filter: Some("^v".into()),
            },
            install: Install::Asset(AssetInstall {
                verify_command: None,
                pattern: Some("Zed-aarch64.dmg".into()),
                binary: None,
                dest: Some("~/Applications/Zed.app".into()),
            }),
        }),
    }
}

fn package_apps() -> Vec<TrackedApp> {
    let app = |id: &str, kind, source| TrackedApp {
        id: id.into(),
        name: id.into(),
        kind,
        auto_update: false,
        update_policy: crate::UpdatePolicy::default(),
        source,
    };
    vec![
        app(
            "claude",
            AppKind::Cli,
            Source::Npm(NpmSource {
                package: "@anthropic-ai/claude-code".into(),
                dist_tag: "latest".into(),
            }),
        ),
        app(
            "vscode",
            AppKind::App,
            Source::Brew(BrewSource {
                package: "visual-studio-code".into(),
                cask: true,
            }),
        ),
        app(
            "rustup",
            AppKind::Cli,
            Source::Script(ScriptSource {
                update_command: "rustup update".into(),
                version_command: Some("rustup --version".into()),
                latest_command: None,
            }),
        ),
    ]
}

#[test]
fn toml_round_trip_keeps_every_source() {
    let mut apps = vec![cli_app("rg"), zed()];
    apps.extend(package_apps());
    let config = Config {
        settings: Settings {
            check_interval_minutes: 15,
            ..Settings::default()
        },
        apps,
    };
    let text = toml::to_string_pretty(&config).unwrap();
    let parsed: Config = toml::from_str(&text).unwrap();
    assert_eq!(parsed, config);
    parsed.validate().unwrap();
    assert!(text.contains("source = \"npm\""), "{text}");
}

#[test]
fn sources_sit_next_to_the_app_fields() {
    let text = r#"
            [[app]]
            id = "rg"
            name = "ripgrep"
            source = "github"
            repo = "BurntSushi/ripgrep"
            track = { mode = "commit" }
            install = { method = "source", build = "cargo build --release", artifact = "target/release/rg" }

            [[app]]
            id = "codex"
            name = "Codex"
            source = "npm"
            package = "@openai/codex"
        "#;
    let config: Config = toml::from_str(text).unwrap();
    assert_eq!(config.settings, Settings::default());
    assert_eq!(config.apps[0].kind, AppKind::Cli);
    assert!(!config.apps[0].auto_update);
    assert!(matches!(&config.apps[1].source, Source::Npm(npm) if npm.dist_tag == "latest"));
    config.validate().unwrap();

    let json = serde_json::to_value(&config.apps[1]).unwrap();
    assert_eq!(json["source"], "npm");
    assert_eq!(json["package"], "@openai/codex");
}

#[test]
fn rejects_duplicate_ids() {
    let config = Config {
        settings: Settings::default(),
        apps: vec![cli_app("rg"), cli_app("rg")],
    };
    assert!(matches!(config.validate(), Err(Error::Config(msg)) if msg.contains("twice")));
}

#[test]
fn legacy_github_configs_load_without_a_source_tag() {
    // Given the original Headway config format.
    let text = r#"[[app]]
id = "rg"
name = "ripgrep"
repo = "BurntSushi/ripgrep"
track = { mode = "commit" }
install = { method = "source", build = "cargo build --release", artifact = "target/release/rg" }
"#;
    // When reading it with the multi-source model.
    let config: Config = toml::from_str(text).unwrap();
    // Then the GitHub source is preserved and unknown explicit tags stay invalid.
    assert_eq!(config.apps[0].github().unwrap().repo, "BurntSushi/ripgrep");
    config.validate().unwrap();
    assert!(
        toml::from_str::<Config>(&text.replace("repo =", "source = \"unsupported\"\nrepo ="))
            .is_err()
    );
}

#[test]
fn asset_installs_require_release_tracking() {
    let app = with_github(cli_app("rg"), |github| GithubSource {
        install: Install::Asset(AssetInstall::default()),
        ..github
    });
    assert!(app.validate().is_err());
    let app = with_github(app, |github| GithubSource {
        track: Track::Release {
            prerelease: false,
            tag_filter: None,
        },
        ..github
    });
    app.validate().unwrap();
}

#[test]
fn source_paths_must_stay_inside_the_repository() {
    for artifact in ["../evil", "/abs/path", ""] {
        let app = with_github(cli_app("x"), |github| GithubSource {
            install: Install::Source(SourceInstall {
                build: "make".into(),
                artifact: Some(artifact.into()),
                ..SourceInstall::default()
            }),
            ..github
        });
        assert!(app.validate().is_err(), "{artifact}");
    }
}

#[test]
fn rejects_bad_fields() {
    let bad = |f: fn(GithubSource) -> GithubSource| with_github(cli_app("x"), f);
    assert!(
        bad(|github| GithubSource {
            track: Track::Tag {
                prerelease: false,
                tag_filter: Some("(".into()),
            },
            ..github
        })
        .validate()
        .is_err()
    );
    assert!(
        bad(|github| GithubSource {
            install: Install::Source(SourceInstall {
                build: "make".into(),
                dest: Some("relative/bin".into()),
                ..SourceInstall::default()
            }),
            ..github
        })
        .validate()
        .is_err()
    );
    assert!(
        bad(|github| GithubSource {
            install: Install::Source(SourceInstall::default()),
            ..github
        })
        .validate()
        .is_err()
    );
    assert!(
        bad(|github| GithubSource {
            track: Track::Commit {
                branch: Some("two words".into()),
            },
            ..github
        })
        .validate()
        .is_err()
    );
}

#[test]
fn package_sources_are_validated() {
    for app in package_apps() {
        app.validate().unwrap();
    }
    let mut broken = package_apps();
    broken[0].source = Source::Npm(NpmSource {
        package: "Not A Package".into(),
        dist_tag: "latest".into(),
    });
    broken[1].source = Source::Brew(BrewSource {
        package: "user/only".into(),
        cask: false,
    });
    broken[2].source = Source::Script(ScriptSource {
        update_command: "  ".into(),
        version_command: None,
        latest_command: None,
    });
    for app in broken {
        assert!(app.validate().is_err(), "{}", app.id);
    }
}

#[test]
fn github_only_operations_say_so() {
    let npm = &package_apps()[0];
    let err = npm.require_github("--branch").unwrap_err();
    assert!(
        matches!(&err, Error::InvalidInput(msg) if msg.contains("npm @anthropic-ai/claude-code"))
    );
    assert!(cli_app("rg").repo_id().is_ok());
}

#[test]
fn settings_keys_parse_from_strings() {
    let base = Settings::default();
    assert_eq!(
        base.with_key("check_interval_minutes", "30")
            .unwrap()
            .check_interval_minutes,
        30
    );
    assert!(!base.with_key("notify", "off").unwrap().notify);
    assert_eq!(base.with_key("bin_dir", "~/bin").unwrap().bin_dir, "~/bin");
    assert!(base.with_key("bin_dir", "bin").is_err());
    assert!(base.with_key("notify", "maybe").is_err());
    assert!(base.with_key("nope", "1").is_err());
    assert_eq!(base, Settings::default(), "with_key must not mutate");
}

#[test]
fn clone_url_prefers_override() {
    let app = cli_app("rg");
    let github = app.github().unwrap();
    assert_eq!(
        github.clone_url().unwrap(),
        "https://github.com/owner/rg.git"
    );
    let ssh = with_github(app, |github| GithubSource {
        git_url: Some("git@github.com:owner/rg.git".into()),
        ..github
    });
    assert_eq!(
        ssh.github().unwrap().clone_url().unwrap(),
        "git@github.com:owner/rg.git"
    );
}

#[test]
fn legacy_auto_update_boolean_keeps_latest_policy() {
    // Given a boolean-only legacy config.
    let text = r#"[[app]]
id = "tool"
name = "Tool"
source = "npm"
package = "tool"
auto_update = true
"#;
    // When reading it without a policy section.
    let config: Config = toml::from_str(text).unwrap();
    // Then legacy automatic updates retain their behavior.
    assert!(config.apps[0].auto_update);
    assert_eq!(config.apps[0].update_policy, UpdatePolicy::default());
    assert!(
        !serde_json::from_str::<crate::state::AppState>("{}")
            .unwrap()
            .auto_update_paused
    );
}

#[test]
fn policy_rejects_invalid_bounds_and_non_versioned_next() {
    // Given policies on each supported source.
    for steps in [0, 21] {
        let mut app = cli_app("tool");
        app.update_policy.max_steps = steps;
        // When validating, then invalid bounds are rejected.
        assert!(app.validate().is_err());
    }
    for mut app in package_apps() {
        app.update_policy.mode = AutoUpdateMode::Next;
        assert_eq!(app.validate().is_ok(), app.source.picks_versions());
    }
}
