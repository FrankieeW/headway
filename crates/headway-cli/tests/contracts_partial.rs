//! Partial previews preserve successful plans without persisting observations.
#![allow(clippy::unwrap_used, clippy::panic)]
mod contract_support;
use contract_support::{add, command, decode, result};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

#[tokio::test]
async fn all_preview_keeps_good_plans_filters_current_apps_and_collects_failures() {
    let home = tempfile::tempdir().unwrap();
    add(home.path(), &[]);
    let marker = home.path().join("updated");
    let update = format!("printf updated > '{}'", marker.display());
    for (name, installed) in [("script:local", "1.0.0"), ("script:current", "2.0.0")] {
        result(
            home.path(),
            &[
                "add",
                name,
                "--no-inspect",
                "--update-command",
                &update,
                "--version-command",
                &format!("printf {installed}"),
                "--latest-command",
                "printf 2.0.0",
            ],
            0,
        );
    }
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let config = std::fs::read(home.path().join("config/config.toml")).unwrap();
    let state = std::fs::read(home.path().join("config/state.json")).ok();
    let preview = decode(
        command(home.path())
            .env("HEADWAY_API_URL", server.uri())
            .args(["--json", "update", "--all", "--dry-run"])
            .output()
            .unwrap(),
        4,
    );
    assert_eq!(
        preview["data"]["plans"].as_array().unwrap().len(),
        1,
        "{preview}"
    );
    assert_eq!(preview["data"]["plans"][0]["app_id"], "local");
    let records = preview["data"]["results"].as_array().unwrap();
    assert!(
        records
            .iter()
            .any(|record| record["app_id"] == "tool" && record["error"]["exit_code"] == 4)
    );
    assert!(
        records
            .iter()
            .any(|record| record["app_id"] == "current" && record["up_to_date"] == true)
    );
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(home.path().join("config/config.toml")).unwrap(),
        config
    );
    assert_eq!(
        std::fs::read(home.path().join("config/state.json")).ok(),
        state
    );
    let real = decode(
        command(home.path())
            .env("HEADWAY_API_URL", server.uri())
            .args(["--json", "update", "--all"])
            .output()
            .unwrap(),
        4,
    );
    assert!(
        marker.exists(),
        "a broken app must not suppress another app's update"
    );
    assert!(
        real["data"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|record| record["app_id"] == "local" && record["ok"] == true)
    );
}

#[test]
fn candidate_destination_conflicts_are_input_errors_but_persisted_invalid_config_is_not() {
    let home = tempfile::tempdir().unwrap();
    let dest = home.path().join("bin/shared");
    let other = home.path().join("bin/other");
    add(
        home.path(),
        &["--artifact", "out/tool", "--dest", dest.to_str().unwrap()],
    );
    let rejected = result(
        home.path(),
        &[
            "add",
            "octo/second",
            "--no-inspect",
            "--build",
            "true",
            "--artifact",
            "out/tool",
            "--dest",
            dest.to_str().unwrap(),
            "--dry-run",
        ],
        2,
    );
    assert_eq!(rejected["error"]["code"], "invalid_input");
    add(
        home.path(),
        &[
            "--id",
            "second",
            "--artifact",
            "out/tool",
            "--dest",
            other.to_str().unwrap(),
        ],
    );
    let rejected = result(
        home.path(),
        &[
            "edit",
            "second",
            "--dest",
            dest.to_str().unwrap(),
            "--dry-run",
        ],
        2,
    );
    assert_eq!(rejected["error"]["code"], "invalid_input");
    let config = home.path().join("config/config.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(text.contains("name = \"tool\""));
    std::fs::write(config, text.replace("name = \"tool\"", "name = \"\"")).unwrap();
    let corrupt = result(
        home.path(),
        &[
            "add",
            "octo/third",
            "--no-inspect",
            "--build",
            "true",
            "--dry-run",
        ],
        1,
    );
    assert_eq!(corrupt["error"]["code"], "invalid_config");
}

#[test]
fn purge_preview_uses_custom_checkout_and_ownership_record() {
    let home = tempfile::tempdir().unwrap();
    let sources = home.path().join("custom-sources");
    add(home.path(), &["--source-root", sources.to_str().unwrap()]);
    let preview = result(home.path(), &["remove", "tool", "--purge", "--dry-run"], 0);
    let targets = preview["data"]["would_remove"].as_array().unwrap();
    assert!(
        targets
            .iter()
            .any(|target| target.as_str().unwrap().ends_with("custom-sources/tool"))
    );
    assert!(targets.iter().any(|target| {
        target
            .as_str()
            .unwrap()
            .ends_with("custom-sources/.tool.headway-source.json")
    }));
    assert!(
        !targets
            .iter()
            .any(|target| target.as_str().unwrap().ends_with("cache/sources/tool"))
    );
}
