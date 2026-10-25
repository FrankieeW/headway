use crate::{
    BrewSource, Error, Headway, Source, Track,
    paths::Paths,
    shell::ShellEnv,
    testing::{FakeTools, cli_app},
    token::SecretStore,
};
use std::sync::Arc;

struct ForbiddenSecrets;
impl SecretStore for ForbiddenSecrets {
    fn get(&self) -> crate::Result<Option<String>> {
        panic!("offline matching must not resolve tokens")
    }
    fn set(&self, _: &str) -> crate::Result<()> {
        panic!("read only")
    }
    fn delete(&self) -> crate::Result<bool> {
        panic!("read only")
    }
}
fn engine(env: ShellEnv) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .env(env)
        .secrets(Arc::new(ForbiddenSecrets))
        .build()
        .unwrap();
    (dir, hw)
}

#[tokio::test]
async fn match_app_ids_tracks_all_github_modes_without_writes() {
    let tools = FakeTools::new().tool("brew", r#"echo '{"formulae":[],"casks":[{"token":"tool","installed":"1","url":"https://github.com/OWNER/tool/releases/a.dmg"}]}'"#);
    let (_dir, hw) = engine(tools.env());
    let mut brew = cli_app("brew-id");
    brew.source = Source::Brew(BrewSource {
        package: "tool".into(),
        cask: true,
        install_path: None,
    });
    let commit = cli_app("tool");
    let mut release = cli_app("release");
    if let Source::Github(github) = &mut release.source {
        github.repo = "owner/tool".into();
        github.track = Track::Release {
            prerelease: false,
            tag_filter: None,
        };
    }
    hw.import_apps(vec![brew, commit, release]).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    let result = hw.match_source("brew-id", true).await.unwrap();
    assert_eq!(result.query, "brew cask tool");
    assert_eq!(result.repositories[0].tracked_by, ["tool", "release"]);
    let inventory = hw.discover_apps(&["brew".into()]).await.unwrap();
    assert_eq!(
        inventory.candidates[0].repositories[0].tracked_by,
        ["tool", "release"]
    );
    assert!(
        inventory.candidates[0]
            .conflicts
            .iter()
            .any(|s| s == "GitHub repository `OWNER/tool` is already tracked by `tool`")
    );
    assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
    assert!(!hw.paths().state_file().exists());
}

#[tokio::test]
async fn match_rejects_npm_script_specs_and_app_ids_before_token_lookup() {
    let (_dir, hw) = engine(FakeTools::new().env());
    let mut npm = cli_app("npm-id");
    npm.source = Source::Npm(crate::NpmSource {
        package: "x".into(),
        dist_tag: "latest".into(),
    });
    let mut script = cli_app("script-id");
    script.source = Source::Script(crate::ScriptSource {
        update_command: "echo ok".into(),
        version_command: None,
        latest_command: None,
    });
    hw.import_apps(vec![npm, script]).unwrap();
    for input in ["npm:x", "script:x", "npm-id", "script-id"] {
        let error = hw.match_source(input, false).await.unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
        assert_eq!(
            error.to_string(),
            "invalid input: match needs owner/repo, a GitHub URL, brew:<formula>, cask:<cask> or the id of a GitHub/Homebrew app"
        );
    }
    let error = hw.match_source("missing-id", true).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        crate::SourceSpec::parse("missing-id")
            .unwrap_err()
            .to_string()
    );
}

#[tokio::test]
async fn github_app_id_matches_tracked_package_and_formula_url_is_accepted() {
    let tools = FakeTools::new().tool("brew", r#"
case "$1" in
search) exit 0 ;;
info) echo '{"formulae":[{"name":"tool","installed":[{"version":"1"}],"homepage":"https://github.com/owner/tool"}],"casks":[]}' ;;
*) exit 99 ;;
esac
"#);
    let (_dir, hw) = engine(tools.env());
    let mut brew = cli_app("brew-id");
    brew.source = Source::Brew(BrewSource {
        package: "tool".into(),
        cask: false,
        install_path: None,
    });
    hw.import_apps(vec![cli_app("tool"), brew]).unwrap();
    let result = hw.match_source("tool", true).await.unwrap();
    assert_eq!(result.packages[0].tracked_by, ["brew-id"]);
    let result = hw
        .match_source("https://formulae.brew.sh/formula/tool", true)
        .await
        .unwrap();
    assert_eq!(result.repositories[0].tracked_by, ["tool"]);
}

#[tokio::test]
async fn github_client_initialization_failure_is_partial_and_keeps_local_evidence() {
    let tools = FakeTools::new().tool(
        "brew",
        r#"echo '{"formulae":[{"name":"tool","homepage":"https://github.com/acme/tool"}]}'"#,
    );
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .env(tools.env().with_var("HEADWAY_GITHUB_TOKEN", "test-token"))
        .api("file:///unusable")
        .secrets(Arc::new(ForbiddenSecrets))
        .build()
        .unwrap();
    let result = hw.match_source("brew:tool", false).await.unwrap();
    assert_eq!(result.repositories[0].repo, "acme/tool");
    assert_eq!(result.errors[0].source, "github");
    assert_eq!(result.errors[0].error.code, "invalid_input");
    assert!(!hw.paths().config_file().exists());
}
