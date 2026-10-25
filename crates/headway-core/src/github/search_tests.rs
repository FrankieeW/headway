use super::GitHub;
use crate::Error;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, path, query_param},
};

#[tokio::test]
async fn repository_search_encodes_query_bounds_page_and_parses_defaults() {
    let server = MockServer::start().await;
    for (limit, expected) in [(0, "1"), (255, "10")] {
        Mock::given(path("/search/repositories")).and(query_param("q", "C++ tool & org:acme/é")).and(query_param("per_page", expected)).and(header("Authorization", "Bearer secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"full_name":"acme/tool", "html_url":"https://github.com/acme/tool"}]})))
            .expect(1).mount(&server).await;
        let gh = GitHub::new(&server.uri(), Some("secret".into())).unwrap();
        let items = gh
            .search_repositories("C++ tool & org:acme/é", limit)
            .await
            .unwrap();
        assert_eq!(items[0].full_name, "acme/tool");
        assert_eq!(items[0].homepage, None);
        assert_eq!(items[0].stargazers_count, 0);
        assert!(!items[0].fork);
        assert!(!items[0].archived);
    }
    for request in server.received_requests().await.unwrap() {
        assert!(!request.url.as_str().contains("secret"));
    }
}

#[tokio::test]
async fn repository_search_classifies_rate_limit() {
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
    assert!(matches!(
        gh.search_repositories("tool", 5).await.unwrap_err(),
        Error::RateLimited { .. }
    ));
}
