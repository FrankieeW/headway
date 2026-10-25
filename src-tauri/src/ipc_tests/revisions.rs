use super::support::{call, commit_json, harness, ok, tool_app};
use serde_json::json;
use tauri::Manager as _;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn storage_repositories_and_restart_admission_use_frontend_payloads() {
    let harness = harness("http://127.0.0.1:9");
    let dest = harness.home.path().join("bin/tool");
    let mut app = tool_app("file:///fixture", &dest);
    app["repositories"] = json!([{"repo":"octo/tool","label":"Official"},{"repo":"fork/tool","label":"My fork","branch":"work"}]);
    ok(&harness, "add_app", json!({"app":app}));
    let storage = ok(&harness, "source_storage_status", json!({"id":"tool"}));
    assert_eq!(storage["keep"], true);
    assert_eq!(storage["exists"], false);
    assert!(storage["path"].as_str().unwrap().ends_with("sources/tool"));
    assert_eq!(
        ok(&harness, "list_repositories", json!({"id":"tool"}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let switched = ok(
        &harness,
        "select_repository",
        json!({"id":"tool","repo":"fork/tool"}),
    );
    assert_eq!(switched["app"]["repo"], "fork/tool");
    assert_eq!(switched["app"]["track"]["branch"], "work");
    assert_eq!(switched["installed"], serde_json::Value::Null);
    let state = harness.app.state::<crate::state::AppState>();
    assert!(state.try_admit_restart_if_idle());
    let error = call(
        &harness,
        "install_revision",
        json!({"id":"tool","request":{"reference":"a".repeat(40),"method":"source"}}),
    )
    .unwrap_err();
    assert_eq!(error["code"], "busy");
    let error=call(&harness,"install_variant",json!({"parentId":"tool","request":{"id":"tool-old","revision":{"reference":"a".repeat(40),"method":"source"}}})).unwrap_err();
    assert_eq!(error["code"], "busy");
    assert_eq!(state.active_runs(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn pages_options_and_independent_dry_run_keep_config_and_payload_unchanged() {
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    Mock::given(path("/repos/octo/tool")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"full_name":"octo/tool","default_branch":"main","html_url":"https://github.com/octo/tool"}))).mount(&server).await;
    Mock::given(path(format!("/repos/octo/tool/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit_json(&sha)))
        .mount(&server)
        .await;
    Mock::given(path("/repos/octo/tool/commits"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "2"))
        .and(query_param("sha", &sha))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([commit_json(&"b".repeat(40))])),
        )
        .mount(&server)
        .await;
    for endpoint in ["tags", "releases"] {
        Mock::given(path(format!("/repos/octo/tool/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
    }
    let harness = harness(&server.uri());
    let bin = harness.home.path().join("bin");
    harness
        .app
        .state::<crate::state::AppState>()
        .hw
        .set_setting("bin_dir", bin.to_str().unwrap())
        .unwrap();
    let dest = bin.join("tool");
    ok(
        &harness,
        "add_app",
        json!({"app":tool_app("file:///fixture",&dest)}),
    );
    let config = harness
        .app
        .state::<crate::state::AppState>()
        .hw
        .paths()
        .config_file();
    let before = std::fs::read(&config).unwrap();
    let page = ok(
        &harness,
        "list_commits",
        json!({"id":"tool","page":2,"perPage":2,"anchor":sha}),
    );
    assert_eq!(page["anchor"], sha);
    assert_eq!(page["commits"].as_array().unwrap().len(), 1);
    assert_eq!(page["has_more"], false);
    let options = ok(
        &harness,
        "revision_options",
        json!({"id":"tool","reference":sha}),
    );
    assert_eq!(options["commit"], sha);
    assert_eq!(options["can_download"], false);
    let plan = ok(
        &harness,
        "plan_variant",
        json!({"parentId":"tool","request":{"id":"tool-old","revision":{"reference":sha,"method":"source"}}}),
    );
    assert_eq!(plan["app_id"], "tool-old");
    assert!(plan["steps"].to_string().contains("bin/tool-old"));
    // A revision plan is read-only as well (no config write, no install).
    let revision = ok(
        &harness,
        "plan_revision",
        json!({"id":"tool","request":{"reference":sha,"method":"source"}}),
    );
    assert_eq!(revision["app_id"], "tool");
    assert!(
        revision["steps"].to_string().contains("bin/tool"),
        "{revision}"
    );
    assert_eq!(std::fs::read(config).unwrap(), before);
    assert!(!dest.exists());
    assert!(!bin.join("tool-old").exists());
    assert_eq!(
        call(&harness, "get_app", json!({"id":"tool-old"})).unwrap_err()["code"],
        "app_not_found"
    );
}

#[test]
fn launch_variant_refuses_apps_without_an_independent_bundle() {
    // A plain tracked CLI has no independent version to launch. The refusal must
    // happen before any program is started.
    let harness = super::support::harness("http://127.0.0.1:9");
    let dest = harness.home.path().join("bin/tool");
    super::support::ok(
        &harness,
        "add_app",
        json!({"app": super::support::tool_app("file:///nowhere", &dest)}),
    );
    for id in ["tool", "missing"] {
        let error = call(&harness, "launch_variant", json!({"id": id})).unwrap_err();
        assert!(
            matches!(
                error["code"].as_str(),
                Some("invalid_input" | "app_not_found")
            ),
            "{id}: {error}"
        );
    }
}
