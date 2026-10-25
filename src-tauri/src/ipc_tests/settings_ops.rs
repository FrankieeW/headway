//! Token, diagnostics and CLI-link commands through the real IPC handler.
//!
//! Secrets go through the harness's in-memory store (never the login keychain),
//! and the GitHub API is a local mock; nothing here touches the network or `gh`.

use serde_json::json;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{bundle_harness, harness_without_token, ok};

async fn github_mock() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "login": "tester", "name": "Test" })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate": { "limit": 5000, "remaining": 4999, "reset": 1_700_000_000_i64 }
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread")]
async fn token_commands_use_the_injected_secret_store() {
    let server = github_mock().await;
    let harness = harness_without_token(&server.uri());
    // `gh` must never be consulted: the fallback is off, so an absent token means
    // "none" instead of shelling out.
    let mut settings = ok(&harness, "get_settings", json!({}));
    settings["gh_fallback"] = json!(false);
    ok(&harness, "save_settings", json!({ "settings": settings }));

    let idle = ok(&harness, "token_status", json!({}));
    assert_eq!(idle["source"], "none");

    let token = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    let saved = ok(&harness, "set_token", json!({ "token": token }));
    assert_eq!(saved["source"], "keychain");
    assert_eq!(saved["login"], "tester");
    assert!(
        saved["masked"]
            .as_str()
            .is_some_and(|masked| masked.ends_with("wxyz")),
        "the token is masked in status: {saved}"
    );

    let cleared = ok(&harness, "clear_token", json!({}));
    assert_eq!(cleared["source"], "none");
    assert!(cleared["masked"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn doctor_reports_structured_checks() {
    let server = github_mock().await;
    let harness = harness_without_token(&server.uri());
    let report = ok(&harness, "doctor", json!({}));
    let checks = report["checks"].as_array().unwrap();
    assert!(!checks.is_empty());
    assert!(report["ok"].is_boolean());
    for check in checks {
        assert!(check["name"].is_string(), "{check}");
        assert!(
            matches!(check["status"].as_str(), Some("ok" | "warn" | "error")),
            "{check}"
        );
        assert!(check["message"].is_string(), "{check}");
    }
}

#[test]
fn install_cli_links_the_bundled_binary() {
    let harness = bundle_harness();
    let status = ok(&harness, "install_cli", json!({}));
    assert_eq!(status["installed"], true);
    assert_eq!(status["needs_repair"], false);
    assert!(harness.home.path().join("bin/headway").is_symlink());
}
