use super::{
    MatchConfidence, MatchSide, SourceMatches, metadata, search, targets::Targets,
    to_brew::github_to_brew,
};
use crate::{CancellationToken, Error, github::GitHub, repo::RepoId, testing::FakeTools};
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[tokio::test]
async fn conflicting_github_urls_override_name_and_homepage_hints() {
    // Given a same-name repository and homepage, but metadata naming other repositories.
    let server = MockServer::start().await;
    Mock::given(path("/repos/someone/fd"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name":"someone/fd", "default_branch":"main", "homepage":"https://fd.dev"
        })))
        .mount(&server)
        .await;
    let gh = GitHub::new(&server.uri(), None).unwrap();
    let mut report = SourceMatches::new("someone/fd".into(), MatchSide::Github);
    let targets = Targets::resolve(
        &RepoId::parse("someone/fd").unwrap(),
        Some(&gh),
        &mut report,
    )
    .await;
    let packages = metadata::parse(
        &json!({"formulae":[
            {"name":"fd", "homepage":"https://fd.dev", "urls":{
                "stable":{"url":"https://github.com/sharkdp/fd/archive/v1.tar.gz"},
                "head":{"url":"https://github.com/another/fd.git"}}},
            {"name":"fd-helper", "homepage":"https://fd.dev", "urls":{
                "stable":{"url":"https://github.com/sharkdp/fd/archive/v1.tar.gz"}}}
        ]})
        .to_string(),
    )
    .unwrap();
    // When search candidates are verified, then contradictory identity drops both hints.
    for package in packages {
        assert!(targets.candidate(package, false).is_none());
    }
}

#[tokio::test]
async fn matching_url_among_other_repositories_keeps_candidate() {
    // Given metadata containing both a matching and an unrelated repository.
    let mut report = SourceMatches::new("someone/fd".into(), MatchSide::Github);
    let targets = Targets::resolve(&RepoId::parse("someone/fd").unwrap(), None, &mut report).await;
    let package = metadata::parse(r#"{"formulae":[{"name":"fd","homepage":"https://github.com/other/fd","urls":{"head":{"url":"https://github.com/SOMEONE/fd.git"}}}]}"#).unwrap().remove(0);
    // When verified, then the intersecting target still proves identity.
    let found = targets.candidate(package, false).unwrap();
    assert_eq!(found.repo, "someone/fd");
    assert_eq!(found.confidence, MatchConfidence::High);
}

#[test]
fn search_names_drop_substring_noise_but_keep_delimited_variants() {
    // Given Homebrew's substring search output, including an unrelated tap formula.
    let output = "frankieew/tap/lmfdb-cli\nfd-find\nlibfd\nfd\nfd@10\nmy-fd\nFD\nfd\n";
    // When ranked, then only exact and delimited names remain in priority order.
    assert_eq!(
        search::rank_names(output, &["fd".into()]),
        ["FD", "fd", "fd@10", "fd-find", "my-fd"]
    );
}

#[tokio::test]
async fn candidate_failures_are_errors_only_for_exact_or_versioned_terms() {
    // Given failed batches and individually broken exact, versioned, prefix and suffix names.
    let tools = FakeTools::new().tool(
        "brew",
        r#"
case "$1:$2" in
info:--installed) echo '{}' ;;
search:*) printf 'vendor/tap/fd\nfd@10\nfd-helper\nhelper-fd\n' ;;
info:--json=v2) echo broken >&2; exit 1 ;;
esac
"#,
    );
    // When both formula and cask metadata fail.
    let report = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        None,
        &RepoId::parse("sharkdp/fd").unwrap(),
        true,
    )
    .await
    .unwrap();
    // Then exact/versioned failures remain partial errors; loose candidates only add notes.
    assert_eq!(report.errors.len(), 4);
    assert!(report.errors.iter().all(
        |e| e.error.message.contains("`vendor/tap/fd`") || e.error.message.contains("`fd@10`")
    ));
    assert_eq!(
        report.notes,
        [
            "Skipped Homebrew formula `fd-helper`: its metadata could not be read.",
            "Skipped Homebrew formula `helper-fd`: its metadata could not be read.",
            "Skipped Homebrew cask `fd-helper`: its metadata could not be read.",
            "Skipped Homebrew cask `helper-fd`: its metadata could not be read.",
        ]
    );
}

#[tokio::test]
async fn missing_individual_metadata_uses_the_same_candidate_failure_policy() {
    // Given a failed batch whose loose candidate is absent from a successful retry.
    let tools = FakeTools::new().tool(
        "brew",
        r#"
case "$*" in
'info --installed --json=v2') echo '{}' ;;
'search --formula fd') printf 'fd\nfd-helper\n' ;;
'search --cask fd') exit 0 ;;
'info --json=v2 --formula fd fd-helper') exit 1 ;;
*) echo '{}' ;;
esac
"#,
    );
    // When individual metadata reads return no packages.
    let report = github_to_brew(
        &CancellationToken::new(),
        &tools.env(),
        None,
        &RepoId::parse("sharkdp/fd").unwrap(),
        true,
    )
    .await
    .unwrap();
    // Then only the exact candidate is an error.
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].error.code, "remote_not_found");
    assert_eq!(
        report.notes,
        ["Skipped Homebrew formula `fd-helper`: its metadata could not be read."]
    );
}

#[test]
fn duplicate_errors_preserve_distinct_sources_codes_and_messages() {
    // Given repeated and distinct provider failures.
    let mut report = SourceMatches::new("acme/tool".into(), MatchSide::Github);
    let error = Error::InvalidInput("first".into());
    // When the same error is recorded again after other failures.
    report.error("brew", &error);
    report.error("github", &error);
    report.error("brew", &Error::InvalidInput("second".into()));
    report.error("brew", &Error::RemoteNotFound("first".into()));
    report.error("brew", &error);
    // Then only identical triples are collapsed, preserving the first occurrence.
    assert_eq!(report.errors.len(), 4);
    assert_eq!(report.errors[0].source, "brew");
    assert_eq!(report.errors[1].source, "github");
    assert_eq!(report.errors[2].error.message, "invalid input: second");
    assert_eq!(report.errors[3].error.code, "remote_not_found");
}
