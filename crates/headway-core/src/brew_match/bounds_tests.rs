use super::{MatchConfidence, to_brew::github_to_brew, to_github::brew_to_github};
use crate::{
    BrewSource, CancellationToken, Error, github::GitHub, repo::RepoId, shell::ShellEnv,
    testing::FakeTools,
};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{path, query_param},
};

#[tokio::test]
async fn missing_brew_is_fatal_for_brew_but_partial_for_github() {
    let env = ShellEnv::from_vars([]).with_var("PATH", "/headway-test-no-tools");
    let source = BrewSource {
        package: "tool".into(),
        cask: false,
        install_path: None,
    };
    assert_eq!(
        brew_to_github(&CancellationToken::new(), &env, None, &source, true)
            .await
            .unwrap_err()
            .code(),
        "io_error"
    );
    let report = github_to_brew(
        &CancellationToken::new(),
        &env,
        None,
        &RepoId::parse("acme/tool").unwrap(),
        true,
    )
    .await
    .unwrap();
    assert!(report.packages.is_empty());
    assert!(
        report
            .errors
            .iter()
            .all(|e| e.source == "brew" && e.error.code == "io_error")
    );
    assert_eq!(report.errors.len(), 1);
}

#[tokio::test]
async fn cancelled_brew_probes_abort_matching() {
    let tools = FakeTools::new().tool("brew", "exec /bin/sleep 30");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let repo = RepoId::parse("acme/tool").unwrap();
    assert!(matches!(
        github_to_brew(&cancel, &tools.env(), None, &repo, true).await,
        Err(Error::Cancelled)
    ));
}

#[tokio::test]
async fn unsafe_repository_name_never_becomes_a_search_argument() {
    let tools = FakeTools::new().tool(
        "brew",
        r#"
[ "$*" = 'info --installed --json=v2' ] || exit 99
echo '{"formulae":[],"casks":[]}'
"#,
    );
    let result = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        None,
        &RepoId::parse("acme/-flag").unwrap(),
        true,
    )
    .await
    .unwrap();
    assert!(result.errors.is_empty());
    assert!(result.packages.is_empty());
}

#[tokio::test]
async fn package_verification_caps_each_kind_and_final_results_at_ten() {
    let names = (0..15).map(|n| format!("tool-{n:02}")).collect::<Vec<_>>();
    let formulas = names
        .iter()
        .take(10)
        .map(|name| json!({"name":name,"homepage":"https://github.com/acme/tool"}))
        .collect::<Vec<_>>();
    let casks = names
        .iter()
        .take(10)
        .map(|name| json!({"token":name,"url":"https://github.com/acme/tool/releases/v1"}))
        .collect::<Vec<_>>();
    let body = format!(
        r#"
case "$1:$2:$3" in
info:--installed:--json=v2) echo '{{"formulae":[],"casks":[]}}' ;;
search:*) printf '%s\n' '{}' ;;
info:--json=v2:--formula) [ "$#" = 13 ] || exit 99; echo '{}' ;;
info:--json=v2:--cask) [ "$#" = 13 ] || exit 99; echo '{}' ;;
*) exit 98 ;;
esac
"#,
        names.join("\n"),
        json!({"formulae":formulas}),
        json!({"casks":casks})
    );
    let tools = FakeTools::new().tool("brew", &body);
    let result = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        None,
        &RepoId::parse("acme/tool").unwrap(),
        true,
    )
    .await
    .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.packages.len(), 10);
    assert!(
        result
            .packages
            .iter()
            .all(|p| p.confidence == MatchConfidence::High)
    );
    assert_eq!(result.packages[0].package, "tool-00");
    assert_eq!(result.packages[9].package, "tool-04");
}

#[tokio::test]
async fn github_search_caps_three_low_results_and_keeps_success_after_another_query_fails() {
    let tools = FakeTools::new().tool(
        "brew",
        r#"echo '{"casks":[{"token":"tool@preview","name":["Tool Editor"]}]}'"#,
    );
    let server = MockServer::start().await;
    let items = (0..8).map(|n| json!({"full_name":format!("acme/tool{n}"),"html_url":format!("https://github.com/acme/tool{n}")})).collect::<Vec<_>>();
    Mock::given(path("/search/repositories"))
        .and(query_param("q", "tool"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":items})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/search/repositories"))
        .and(query_param("q", "Tool Editor"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let source = BrewSource {
        package: "tool@preview".into(),
        cask: true,
        install_path: None,
    };
    let report = brew_to_github(
        &CancellationToken::new(),
        &tools.env(),
        Some(&gh),
        &source,
        false,
    )
    .await
    .unwrap();
    assert_eq!(report.repositories.len(), 3);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].source, "github");
    assert!(
        report
            .repositories
            .iter()
            .all(|r| r.confidence == MatchConfidence::Low)
    );
}
