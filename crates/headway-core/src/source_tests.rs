use super::*;

#[test]
fn parses_every_spelling_of_every_source() {
    let npm = |name: &str| SourceSpec::Npm(name.to_owned());
    let brew = |name: &str, cask| SourceSpec::Brew {
        package: name.to_owned(),
        cask,
    };
    let cases = [
        (
            "npm:@anthropic-ai/claude-code",
            npm("@anthropic-ai/claude-code"),
        ),
        (
            "https://www.npmjs.com/package/@openai/codex",
            npm("@openai/codex"),
        ),
        ("https://www.npmjs.com/package/pnpm/v/9.0.0", npm("pnpm")),
        ("brew:ripgrep", brew("ripgrep", false)),
        ("cask:visual-studio-code", brew("visual-studio-code", true)),
        (
            "brew:frankieew/tap/headway",
            brew("frankieew/tap/headway", false),
        ),
        (
            "https://formulae.brew.sh/formula/ripgrep",
            brew("ripgrep", false),
        ),
        ("https://formulae.brew.sh/cask/zed/", brew("zed", true)),
        ("script:claude", SourceSpec::Script("claude".into())),
        (
            "https://github.com/BurntSushi/ripgrep",
            SourceSpec::Github(RepoId::parse("BurntSushi/ripgrep").unwrap()),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(SourceSpec::parse(input).unwrap(), expected, "{input}");
    }
    for bad in ["npm:Bad Name", "brew:a/b", "script:", "nonsense", "npm:"] {
        assert!(SourceSpec::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn npm_and_brew_names() {
    assert!(is_npm_package("left-pad"));
    assert!(is_npm_package("@scope/pkg.js"));
    assert!(!is_npm_package("_private"));
    assert!(!is_npm_package("@scope"));
    assert!(!is_npm_package("UPPER"));
    assert!(is_brew_name("python@3.13"));
    assert!(is_brew_name("user/tap/thing"));
    assert!(!is_brew_name("user/thing"));
    assert!(!is_brew_name("a b"));
}

#[test]
fn package_inputs_cannot_be_mistaken_for_options_or_other_hosts() {
    // Given malformed names, option-like values and misleading URL hosts.
    for input in [
        "npm:--registry",
        "brew:--prefix",
        "brew:..",
        "cask:user/tap/--help",
        "https://example.invalid/npmjs.com/package/tool",
        "https://npmjs.com.example.invalid/package/tool",
    ] {
        // When parsing a source.
        let result = SourceSpec::parse(input);
        // Then it cannot become a package-manager argument.
        assert!(result.is_err(), "{input}");
    }
}

#[test]
fn labels_and_links() {
    let npm = Source::Npm(NpmSource {
        package: "@openai/codex".into(),
        dist_tag: "latest".into(),
    });
    assert_eq!(npm.label(), "npm @openai/codex");
    assert_eq!(
        npm.web_url().as_deref(),
        Some("https://www.npmjs.com/package/@openai/codex")
    );
    let cask = Source::Brew(BrewSource {
        package: "zed".into(),
        cask: true,
    });
    assert_eq!(
        cask.web_url().as_deref(),
        Some("https://formulae.brew.sh/cask/zed")
    );
    let tap = Source::Brew(BrewSource {
        package: "frankieew/tap/headway".into(),
        cask: false,
    });
    assert_eq!(tap.web_url(), None);
    assert!(!tap.picks_versions() && tap.detects_installed());
}

#[test]
fn github_ssh_sources_and_urls_with_query_strings_keep_the_repository() {
    // Given the supported SSH spellings and a URL copied with a tracking query.
    for input in [
        "git@github.com:owner/tool.git",
        "ssh://git@github.com/owner/tool.git",
        "https://github.com/owner/tool?tab=readme",
    ] {
        // When parsing a source.
        let spec = SourceSpec::parse(input).unwrap();
        // Then the original repository identity survives.
        assert_eq!(
            spec,
            SourceSpec::Github(RepoId::parse("owner/tool").unwrap())
        );
    }
}

#[test]
fn github_identity_uses_canonical_case_insensitive_repo_and_track_mode() {
    // Given equivalent GitHub spellings and a distinct tracking mode.
    let source = |repo: &str, track| {
        Source::Github(GithubSource {
            options: Box::default(),
            repo: repo.into(),
            git_url: None,
            track,
            install: crate::config::Install::Source(crate::config::SourceInstall {
                build: "true".into(),
                ..crate::config::SourceInstall::default()
            }),
        })
    };
    let base = source(
        "Frankieew/headway",
        crate::config::Track::Commit { branch: None },
    );
    for repo in [
        "FrankieeW/Headway",
        "https://github.com/FRANKIEEW/HEADWAY.git",
        "git@github.com:FrankieeW/headway.git",
    ] {
        // When comparing parsed repository identity with the same track mode.
        let equivalent = source(
            repo,
            crate::config::Track::Commit {
                branch: Some("main".into()),
            },
        );
        // Then URL syntax, casing and branch selection do not create another identity.
        assert!(base.same_identity(&equivalent));
    }
    assert!(!base.same_identity(&source(
        "FrankieeW/headway",
        crate::config::Track::Release {
            prerelease: false,
            tag_filter: None
        }
    )));
    assert!(!base.same_identity(&source(
        "other/headway",
        crate::config::Track::Commit { branch: None }
    )));
}
