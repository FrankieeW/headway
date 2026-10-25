use super::{
    MatchConfidence, MatchEvidenceKind as Kind, to_brew::github_to_brew, to_github::brew_to_github,
};
use crate::{BrewSource, CancellationToken, github::GitHub, repo::RepoId, testing::FakeTools};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

fn source(cask: bool) -> BrewSource {
    BrewSource {
        package: "tool".into(),
        cask,
        install_path: None,
    }
}

#[tokio::test]
async fn brew_urls_skip_search_and_strip_tokens_from_process_environment() {
    let tools = FakeTools::new().tool(
        "brew",
        r#"
[ "$HOMEBREW_NO_AUTO_UPDATE:$HOMEBREW_NO_ENV_HINTS:$HOMEBREW_NO_ANALYTICS" = 1:1:1 ] || exit 91
[ -z "$HEADWAY_GITHUB_TOKEN$GITHUB_TOKEN$GH_TOKEN" ] || exit 92
[ "$*" = 'info --json=v2 --cask tool' ] || exit 93
echo '{"casks":[{"token":"tool","url":"https://github.com/Acme/tool/releases/a.dmg"}]}'
"#,
    );
    let server = MockServer::start().await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let env = tools
        .env()
        .with_var("GH_TOKEN", "secret")
        .with_var("HEADWAY_GITHUB_TOKEN", "secret");
    let result = brew_to_github(
        &CancellationToken::new(),
        &env,
        Some(&gh),
        &source(true),
        false,
    )
    .await
    .unwrap();
    assert_eq!(result.repositories[0].repo, "Acme/tool");
    assert_eq!(result.repositories[0].confidence, MatchConfidence::High);
    assert!(result.notes.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn brew_search_merges_display_name_results_and_homepage_evidence() {
    let tools = FakeTools::new().tool("brew", r#"echo '{"casks":[{"token":"tool","name":["Tool Editor"],"homepage":"https://www.tool.dev/"}]}'"#);
    let server = MockServer::start().await;
    for term in ["tool", "Tool Editor"] {
        Mock::given(method("GET")).and(path("/search/repositories")).and(query_param("q", term)).and(query_param("per_page", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"full_name":"acme/tool","html_url":"https://github.com/acme/tool","homepage":"http://tool.dev","description":"Editor","stargazers_count":42}]})))
            .expect(1).mount(&server).await;
    }
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let result = brew_to_github(
        &CancellationToken::new(),
        &tools.env(),
        Some(&gh),
        &source(true),
        false,
    )
    .await
    .unwrap();
    assert_eq!(result.repositories.len(), 1);
    let found = &result.repositories[0];
    assert_eq!(found.confidence, MatchConfidence::Medium);
    assert_eq!(found.stars, Some(42));
    assert_eq!(found.description.as_deref(), Some("Editor"));
    assert_eq!(
        found.evidence.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::HomepageMatch, Kind::Name, Kind::Search, Kind::Search]
    );
}

#[tokio::test]
async fn brew_offline_note_and_search_failure_keep_report() {
    let tools = FakeTools::new().tool(
        "brew",
        r#"echo '{"formulae":[{"name":"tool","homepage":"https://tool.dev"}]}'"#,
    );
    let cancel = CancellationToken::new();
    let offline = brew_to_github(&cancel, &tools.env(), None, &source(false), true)
        .await
        .unwrap();
    assert_eq!(offline.notes, ["GitHub search skipped (offline)."]);
    let server = MockServer::start().await;
    Mock::given(path("/search/repositories"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .set_body_json(json!({"message":"rate limit"})),
        )
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let result = brew_to_github(&cancel, &tools.env(), Some(&gh), &source(false), false)
        .await
        .unwrap();
    assert_eq!(result.errors[0].source, "github");
    assert_eq!(result.errors[0].error.code, "rate_limited");
    assert_eq!(result.query, "brew tool");
}

#[tokio::test]
async fn unreadable_brew_input_is_fatal_and_unknown_package_is_not_found() {
    for (body, code) in [
        (
            "echo 'Error: No available formula' >&2; exit 1",
            "remote_not_found",
        ),
        ("echo failed >&2; exit 3", "command_failed"),
        ("echo '{'", "parse_error"),
    ] {
        let tools = FakeTools::new().tool("brew", body);
        let error = brew_to_github(
            &CancellationToken::new(),
            &tools.env(),
            None,
            &source(false),
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), code);
    }
}

#[tokio::test]
async fn github_installed_renames_search_verification_and_batch_retry() {
    let tools = FakeTools::new().tool("brew", r#"
case "$*" in
'info --installed --json=v2') echo '{"formulae":[{"name":"gh","installed":[{"version":"2.0"}],"urls":{"stable":{"url":"https://github.com/cli/cli/archive/v2.tar.gz"}}}]}' ;;
'search --formula cli') printf '==> Formulae\ncli\ncli-broken\nnot-cli\n' ;;
'search --cask cli') echo 'Error: No formulae or casks found for "cli".' >&2; exit 1 ;;
'info --json=v2 --formula cli cli-broken not-cli') exit 1 ;;
'info --json=v2 --formula cli') echo '{"formulae":[{"name":"cli"}]}' ;;
'info --json=v2 --formula cli-broken') echo 'No available formula' >&2; exit 1 ;;
'info --json=v2 --formula not-cli') echo '{"formulae":[{"name":"not-cli","homepage":"https://example.org"}]}' ;;
*) echo "unexpected $*" >&2; exit 95 ;;
esac
"#);
    let result = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        None,
        &RepoId::parse("cli/cli").unwrap(),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        result
            .packages
            .iter()
            .map(|m| m.package.as_str())
            .collect::<Vec<_>>(),
        ["gh", "cli"]
    );
    assert_eq!(result.packages[0].installed.as_deref(), Some("2.0"));
    assert_eq!(result.packages[1].confidence, MatchConfidence::Low);
    assert!(result.errors.is_empty());
    assert_eq!(
        result.notes,
        ["Skipped Homebrew formula `cli-broken`: its metadata could not be read."]
    );
}

#[tokio::test]
async fn github_uses_canonical_identity_parent_source_and_homepage() {
    let tools = FakeTools::new().tool("brew", r#"
case "$1:$2" in
info:--installed) echo '{"formulae":[{"name":"parent","installed":[{"version":"1"}],"homepage":"https://github.com/upstream/tool"}],"casks":[{"token":"root","installed":"2","url":"https://github.com/root/tool/releases/v2"}]}' ;;
search:--formula) echo tool ;;
search:--cask) exit 0 ;;
info:--json=v2) echo '{"formulae":[{"name":"tool","homepage":"https://www.tool.dev/"}]}' ;;
*) exit 96 ;;
esac
"#);
    let server = MockServer::start().await;
    Mock::given(path("/repos/old/tool")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "full_name":"Canonical/Tool","default_branch":"main","html_url":"https://github.com/Canonical/Tool", "homepage":"http://tool.dev", "fork":true,
        "parent":{"full_name":"upstream/tool","default_branch":"main"}, "source":{"full_name":"root/tool","default_branch":"main"}
    }))).mount(&server).await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let result = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        Some(&gh),
        &RepoId::parse("old/tool").unwrap(),
        false,
    )
    .await
    .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.packages.len(), 3);
    assert_eq!(result.packages[0].repo, "upstream/tool");
    assert_eq!(result.packages[1].repo, "root/tool");
    assert_eq!(result.packages[2].repo, "Canonical/Tool");
    assert_eq!(result.packages[2].confidence, MatchConfidence::Medium);
}

#[tokio::test]
async fn github_failure_and_brew_search_failure_preserve_installed_matches() {
    let tools = FakeTools::new().tool("brew", r#"
case "$2" in
--installed) echo '{"formulae":[{"name":"tool","installed":[{"version":"1"}],"homepage":"https://github.com/acme/tool"}]}' ;;
*) echo broken >&2; exit 8 ;;
esac
"#);
    let gh = GitHub::new("http://127.0.0.1:1", None).unwrap();
    let result = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        Some(&gh),
        &RepoId::parse("acme/tool").unwrap(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(result.packages.len(), 1);
    assert_eq!(result.errors[0].source, "github");
    assert!(result.errors.iter().any(|e| e.source == "brew"));
}
