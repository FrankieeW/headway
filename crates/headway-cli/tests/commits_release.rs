//! CLI contracts for the release-aware commit listing and the install method
//! that a single update run can force.
#![allow(clippy::unwrap_used)]
use assert_cmd::Command;
use serde_json::{Value, json};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const RELEASED: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PLAIN: &str = "cccccccccccccccccccccccccccccccccccccccc";
const TIP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TAG: &str = "v1.2.3";
const ASSET: &str = "tool-v1.2.3.tar.gz";

fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("headway"));
    command
        .env("HEADWAY_HOME", root)
        .env("HEADWAY_API_URL", "http://127.0.0.1:1")
        .env("HEADWAY_GITHUB_TOKEN", "test-token")
        .env_remove("HEADWAY_OUTPUT")
        .env("NO_COLOR", "1");
    command
}

fn document(root: &std::path::Path, args: &[&str]) -> Value {
    let output = command(root)
        .arg("--json")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

fn commit(sha: &str, message: &str) -> Value {
    json!({"sha":sha,"html_url":format!("https://github.com/octo/tool/commit/{sha}"),
        "commit":{"message":message,"author":{"name":"Builder","date":"2026-10-01T00:00:00Z"},
            "committer":{"date":"2026-10-02T00:00:00Z"}}})
}

/// The repository, its branch tip, one released commit and one plain commit.
async fn repository(server: &MockServer) {
    let respond = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    Mock::given(path("/repos/octo/tool"))
        .respond_with(respond(json!({
            "full_name":"octo/tool","default_branch":"main","html_url":"https://github.com/octo/tool"
        })))
        .mount(server)
        .await;
    Mock::given(path("/repos/octo/tool/commits/main"))
        .respond_with(respond(commit(TIP, "Tip")))
        .mount(server)
        .await;
    Mock::given(path("/repos/octo/tool/commits"))
        .respond_with(
            respond(json!([
                commit(RELEASED, "Published"),
                commit(PLAIN, "Plain")
            ]))
            .insert_header(
                "link",
                "<https://api.github.com/repositories/1/commits?page=2>; rel=\"next\"",
            ),
        )
        .mount(server)
        .await;
    Mock::given(path("/repos/octo/tool/tags"))
        .respond_with(respond(json!([{"name":TAG,"commit":{"sha":RELEASED}}])))
        .mount(server)
        .await;
    Mock::given(path("/repos/octo/tool/releases"))
        .respond_with(respond(json!([{
            "id":7,"tag_name":TAG,"draft":false,"prerelease":false,
            "html_url":"https://github.com/octo/tool/releases/tag/v1.2.3",
            "assets":[{"id":9,"name":ASSET,"size":10}]
        }])))
        .mount(server)
        .await;
}

#[test]
fn the_command_contract_lists_the_release_filters_and_the_install_method() {
    // Given the machine-readable command description.
    let root = tempfile::tempdir().unwrap();
    let described = document(root.path(), &["commands"]);
    let commands = described["data"]["subcommands"].as_array().unwrap();
    let flag = |name: &str, flag: &str| -> Option<&Value> {
        commands
            .iter()
            .find(|command| command["name"] == name)
            .and_then(|command| command["args"].as_array())
            .and_then(|args| args.iter().find(|arg| arg["flag"] == flag))
    };
    // Then the commit filters and the install method are part of the contract.
    assert!(flag("commits", "--release").is_some());
    assert!(flag("commits", "--tag").is_some());
    let ci = flag("commits", "--ci").expect("the CI filter");
    assert_eq!(
        ci["possible_values"],
        json!(["success", "failure", "pending", "not-run", "unknown"])
    );
    let method = flag("update", "--method").expect("the install method");
    assert_eq!(
        method["possible_values"],
        json!(["configured", "source", "asset"])
    );
    assert_eq!(method["default"], "configured");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_release_filter_narrows_the_commit_listing() {
    // Given a repository whose newest commit published a release.
    let root = tempfile::tempdir().unwrap();
    document(
        root.path(),
        &["add", "octo/tool", "--no-inspect", "--build", "true"],
    );
    let server = MockServer::start().await;
    repository(&server).await;
    let invoke = |args: &[&str]| {
        let output = command(root.path())
            .env("HEADWAY_API_URL", server.uri())
            .arg("--json")
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice::<Value>(&output).unwrap()
    };
    // When the history is listed without a filter and with one.
    let all = invoke(&["commits", "tool"]);
    let releases = invoke(&["commits", "tool", "--release"]);
    // Then the unfiltered page names both commits and the filtered one only the release.
    assert_eq!(all["data"]["commits"].as_array().unwrap().len(), 2);
    assert_eq!(all["data"]["commits"][0]["release_tag"], TAG);
    assert_eq!(all["data"]["commits"][1]["release_tag"], Value::Null);
    assert_eq!(releases["data"]["commits"].as_array().unwrap().len(), 1);
    assert_eq!(releases["data"]["commits"][0]["id"], RELEASED);
    assert_eq!(releases["data"]["filtered_out"], 1);
    assert_eq!(releases["data"]["filter"]["releases_only"], true);
    // And the next hint keeps the filter.
    let next = releases["next"].as_array().unwrap();
    assert!(
        next.iter()
            .any(|hint| hint.as_str().unwrap_or_default().contains("--release")),
        "{next:?}"
    );
    // And the human output names the release next to the commit.
    let human = command(root.path())
        .env("HEADWAY_API_URL", server.uri())
        .args(["commits", "tool", "--release"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(human).unwrap();
    assert!(text.contains(TAG), "{text}");
    assert!(text.contains("Published"), "{text}");
    // And the XML output carries the same release information.
    let xml = command(root.path())
        .env("HEADWAY_API_URL", server.uri())
        .args(["--ai", "commits", "tool", "--release"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(xml).unwrap();
    assert!(text.contains(TAG), "{text}");
    assert!(text.contains("release_tag"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tag_filter_rejects_a_broken_pattern_without_a_request() {
    // Given a repository without any fixture.
    let root = tempfile::tempdir().unwrap();
    document(
        root.path(),
        &["add", "octo/tool", "--no-inspect", "--build", "true"],
    );
    let server = MockServer::start().await;
    // When the history is narrowed by a broken pattern.
    let output = command(root.path())
        .env("HEADWAY_API_URL", server.uri())
        .arg("--json")
        .args(["commits", "tool", "--tag", "v1("])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let result: Value = serde_json::from_slice(&output).unwrap();
    // Then the request is rejected locally.
    assert_eq!(result["error"]["code"], "invalid_input");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn forced_asset_dry_run_preserves_method_in_the_execution_hint() {
    // Given a source-tracked app whose selected commit published a release.
    let root = tempfile::tempdir().unwrap();
    document(
        root.path(),
        &["add", "octo/tool", "--no-inspect", "--build", "true"],
    );
    let server = MockServer::start().await;
    repository(&server).await;
    for reference in [RELEASED.to_owned(), format!("refs/tags/{TAG}")] {
        Mock::given(path(format!("/repos/octo/tool/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(commit(RELEASED, "Published")))
            .mount(&server)
            .await;
    }
    Mock::given(path(format!("/repos/octo/tool/releases/tags/{TAG}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":7,"tag_name":TAG,"assets":[{"id":9,"name":ASSET,"size":10}]
        })))
        .mount(&server)
        .await;

    // When the actual CLI plans a one-off release download.
    let output = command(root.path())
        .env("HEADWAY_API_URL", server.uri())
        .args([
            "--json",
            "update",
            "tool",
            "--to",
            RELEASED,
            "--method",
            "asset",
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let result: Value = serde_json::from_slice(&output).unwrap();

    // Then the plan and executable next hint both retain the chosen method.
    assert_eq!(result["data"]["plans"][0]["method"], "asset");
    assert_eq!(result["data"]["plans"][0]["asset"]["name"], ASSET);
    let next = result["next"][0].as_str().unwrap();
    assert!(next.contains("--method asset"), "{next}");
    assert!(!next.contains("--dry-run"), "{next}");
    let app = document(root.path(), &["show", "tool"]);
    assert_eq!(app["data"]["app"]["install"]["method"], "source");
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().contains("/assets/"))
    );
}
