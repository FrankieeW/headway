use super::{MatchConfidence, MatchEvidenceKind, SourceMatches, to_github::brew_to_github};
use crate::{BrewSource, CancellationToken, github::GitHub, testing::FakeTools};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{path, query_param},
};

async fn search_results(first: Vec<Value>, second: Vec<Value>, homepage: &str) -> SourceMatches {
    let metadata =
        json!({"casks":[{"token":"github", "name":["GitHub Desktop"], "homepage":homepage}]});
    let tools = FakeTools::new().tool("brew", &format!("echo '{metadata}'"));
    let server = MockServer::start().await;
    for (term, items) in [("github", first), ("GitHub Desktop", second)] {
        Mock::given(path("/search/repositories"))
            .and(query_param("q", term))
            .and(query_param("per_page", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":items})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let gh = GitHub::new(&server.uri(), None).unwrap();
    brew_to_github(
        &CancellationToken::new(),
        &tools.env(),
        Some(&gh),
        &BrewSource {
            package: "github".into(),
            cask: true,
            install_path: None,
        },
        false,
    )
    .await
    .unwrap()
}

fn item(repo: &str, homepage: Option<&str>) -> Value {
    json!({"full_name":repo, "html_url":format!("https://github.com/{repo}"), "homepage":homepage})
}

#[tokio::test]
async fn low_results_interleave_queries_dedupe_and_cap_at_three() {
    // Given five weak token hits and a different top display-name hit, with a shared second hit.
    let first = vec![
        item("a/github", None),
        item("a/shared", None),
        item("a/third", None),
        item("a/fourth", None),
        item("a/fifth", None),
    ];
    let second = vec![
        item("desktop/desktop", None),
        item("A/shared", None),
        item("b/third", None),
    ];
    // When both queries are merged.
    let report = search_results(first, second, "https://desktop.example").await;
    // Then rank interleaving survives the cap and duplicate evidence is merged.
    assert_eq!(
        report
            .repositories
            .iter()
            .map(|r| r.repo.as_str())
            .collect::<Vec<_>>(),
        ["a/github", "desktop/desktop", "a/shared"]
    );
    assert!(
        report
            .repositories
            .iter()
            .all(|r| r.confidence == MatchConfidence::Low)
    );
    assert_eq!(
        report.repositories[2]
            .evidence
            .iter()
            .filter(|e| e.kind == MatchEvidenceKind::Search)
            .count(),
        2
    );
}

#[tokio::test]
async fn homepage_matches_drop_low_results_and_keep_stable_interleaved_order() {
    // Given mixed-confidence hits in different queries; one repeated hit gains homepage evidence.
    let homepage = "https://desktop.example";
    let first = vec![
        item("a/weak", None),
        item("a/strong", Some(homepage)),
        item("a/upgraded", None),
    ];
    let second = vec![
        item("desktop/desktop", Some(homepage)),
        item("A/upgraded", Some(homepage)),
        item("b/weak", None),
    ];
    // When the full result set is deduplicated and sorted by confidence.
    let report = search_results(first, second, homepage).await;
    // Then medium hits retain interleaved ordering, including the upgraded duplicate, without low noise.
    assert_eq!(
        report
            .repositories
            .iter()
            .map(|r| r.repo.as_str())
            .collect::<Vec<_>>(),
        ["desktop/desktop", "a/strong", "A/upgraded"]
    );
    assert!(
        report
            .repositories
            .iter()
            .all(|r| r.confidence == MatchConfidence::Medium)
    );
    assert_eq!(
        report.repositories[2]
            .evidence
            .iter()
            .filter(|e| e.kind == MatchEvidenceKind::Search)
            .count(),
        2
    );
}

#[tokio::test]
async fn medium_results_remain_capped_at_five() {
    // Given ten distinct homepage-evidenced hits across both queries.
    let homepage = "https://desktop.example";
    let first = (0..5)
        .map(|n| item(&format!("a/tool{n}"), Some(homepage)))
        .collect();
    let second = (0..5)
        .map(|n| item(&format!("b/tool{n}"), Some(homepage)))
        .collect();
    // When merged, then the existing five-result cap applies after interleaving.
    let report = search_results(first, second, homepage).await;
    assert_eq!(
        report
            .repositories
            .iter()
            .map(|r| r.repo.as_str())
            .collect::<Vec<_>>(),
        ["a/tool0", "b/tool0", "a/tool1", "b/tool1", "a/tool2"]
    );
}
