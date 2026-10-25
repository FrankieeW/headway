use super::support::{call, commit_json, harness, ok, tool_app};
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[tokio::test(flavor = "multi_thread")]
async fn installation_fork_discovery_and_pr_revision_use_frontend_contracts() {
    // Given a fork, its parent, and an exact PR head.
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    let pull = json!({"number":7,"title":"PR build","html_url":"https://github.com/octo/tool/pull/7","state":"open",
        "head":{"ref":"feature","sha":sha,"repo":{"full_name":"fork/tool"}},"base":{"ref":"main","sha":"b".repeat(40),"repo":{"full_name":"octo/tool"}}});
    for (endpoint, body) in [
        (
            String::new(),
            json!({"full_name":"octo/tool","default_branch":"main","html_url":"https://github.com/octo/tool","fork":true,"parent":{"full_name":"upstream/tool","default_branch":"develop"},"source":{"full_name":"upstream/tool","default_branch":"develop"}}),
        ),
        ("/pulls".to_owned(), json!([pull.clone()])),
        ("/pulls/7".to_owned(), pull),
        (format!("/commits/{sha}"), commit_json(&sha)),
    ] {
        Mock::given(path(format!("/repos/octo/tool{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    let harness = harness(&server.uri());
    let dest = harness.home.path().join("bin/tool");
    ok(
        &harness,
        "add_app",
        json!({"app":tool_app("file:///fixture", &dest)}),
    );
    // When the renderer invokes detection, fork discovery and PR preview.
    let installation = ok(&harness, "detect_installation", json!({"id":"tool"}));
    let refreshed = ok(&harness, "refresh_repositories", json!({"id":"tool"}));
    let page = ok(
        &harness,
        "list_pull_requests",
        json!({"id":"tool","page":1,"perPage":20}),
    );
    let options = ok(
        &harness,
        "revision_options",
        json!({"id":"tool","reference":"pr:7"}),
    );
    // Then the wire shapes carry exact evidence without installing or activating the parent.
    assert_eq!(installation["presence"], "missing");
    assert_eq!(installation["baseline_matches"], false);
    assert_eq!(refreshed["app"]["repo"], "octo/tool");
    assert_eq!(
        refreshed["app"]["repositories"].as_array().unwrap().len(),
        2
    );
    assert_eq!(page["pull_requests"][0]["head"]["sha"], sha);
    assert_eq!(options["reference"], format!("pr:7@{sha}"));
    assert_eq!(options["pull_request"]["number"], 7);
    assert_eq!(options["can_download"], false);
    assert_eq!(
        call(
            &harness,
            "list_pull_requests",
            json!({"id":"tool","page":0,"perPage":20})
        )
        .unwrap_err()["code"],
        "invalid_input"
    );
    assert!(!dest.exists());
}
