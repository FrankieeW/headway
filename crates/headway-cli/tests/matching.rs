//! End-to-end matching with an isolated HOME and read-only fake brew.
#![allow(clippy::unwrap_used, clippy::panic)]
mod source_support;
use serde_json::Value;
use source_support::Sandbox;

fn sandbox() -> Sandbox {
    let s = Sandbox::new();
    s.tool("brew", r#"
case "$*" in
'info --json=v2 --cask tool') echo '{"casks":[{"token":"tool","name":["Tool"],"version":"1","url":"https://github.com/acme/tool/releases/a.dmg"}]}' ;;
'info --json=v2 --cask unknown') echo '{"casks":[{"token":"unknown","homepage":"https://unknown.dev"}]}' ;;
'info --installed --json=v2') echo '{"formulae":[{"name":"renamed","installed":[{"version":"1"}],"homepage":"https://github.com/acme/tool"}],"casks":[]}' ;;
'search --formula tool') echo tool ;;
'search --cask tool') exit 0 ;;
'info --json=v2 --formula tool') echo '{"formulae":[{"name":"tool","homepage":"https://github.com/acme/tool"}]}' ;;
*) exit 99 ;;
esac
"#);
    s
}

#[test]
fn match_cask_offline_returns_download_evidence_and_add_hint() {
    let s = sandbox();
    let reply = s.ok(&["match", "cask:tool", "--offline"]);
    let item = &reply["data"]["repositories"][0];
    assert_eq!(reply["data"]["from"], "brew");
    assert_eq!(item["repo"], "acme/tool");
    assert_eq!(item["confidence"], "high");
    assert_eq!(item["evidence"][0]["kind"], "download_url");
    assert_eq!(item["description"], Value::Null);
    assert_eq!(item["stars"], Value::Null);
    assert_eq!(reply["next"][0], "headway --json add acme/tool");
    s.command()
        .args(["--json", "match", "cask:tool", "--offline"])
        .assert()
        .code(0);
    let human = s
        .command()
        .args(["match", "cask:tool", "--offline"])
        .output()
        .unwrap();
    assert!(human.status.success());
    assert!(String::from_utf8_lossy(&human.stdout).contains("brew cask tool → GitHub"));
    let xml = s
        .command()
        .args(["--ai", "match", "cask:tool", "--offline"])
        .output()
        .unwrap();
    assert!(xml.status.success());
    assert!(String::from_utf8_lossy(&xml.stdout).contains("download_url"));
    s.unchanged();
}

#[test]
fn match_github_offline_combines_installed_and_verified_search() {
    let s = sandbox();
    let reply = s.ok(&["match", "acme/tool", "--offline"]);
    assert_eq!(reply["data"]["from"], "github");
    assert_eq!(reply["data"]["packages"][0]["package"], "renamed");
    assert_eq!(reply["data"]["packages"][0]["installed"], "1");
    assert_eq!(reply["data"]["packages"][1]["package"], "tool");
    s.unchanged();
}

#[test]
fn match_npm_rejects_with_invalid_input_exit_two() {
    let s = sandbox();
    s.command()
        .args(["--json", "match", "npm:x"])
        .assert()
        .code(2);
    assert_eq!(s.run(&["match", "npm:x"])["error"]["code"], "invalid_input");
    s.unchanged();
}

#[test]
fn match_search_failure_returns_partial_data_exit_one() {
    let s = sandbox();
    s.command()
        .args(["--json", "match", "cask:unknown"])
        .assert()
        .code(1);
    let reply = s.run(&["match", "cask:unknown"]);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["data"]["query"], "brew cask unknown");
    assert_eq!(reply["data"]["errors"][0]["source"], "github");
    assert_eq!(reply["data"]["repositories"], serde_json::json!([]));
    s.unchanged();
}

#[test]
fn match_tracked_brew_app_id_is_read_only() {
    let s = sandbox();
    s.ok(&["add", "cask:tool", "--id", "custom-tool"]);
    let before = std::fs::read(s.dir.path().join("config/config.toml")).unwrap();
    let reply = s.ok(&["match", "custom-tool", "--offline"]);
    assert_eq!(reply["data"]["repositories"][0]["repo"], "acme/tool");
    assert_eq!(
        std::fs::read(s.dir.path().join("config/config.toml")).unwrap(),
        before
    );
    s.unchanged();
}

#[test]
fn commands_catalog_lists_match_and_offline() {
    let s = sandbox();
    let reply = s.ok(&["commands"]);
    let text = reply["data"].to_string();
    assert!(text.contains("\"match\""));
    assert!(text.contains("--offline"));
}

#[test]
fn local_url_evidence_succeeds_when_github_client_cannot_be_created() {
    // Given a package with local URL evidence and an invalid API URL.
    let s = sandbox();
    // When matching online, then an unused client failure does not affect exit/JSON status.
    let output = s
        .command()
        .env("HEADWAY_API_URL", "file:///unusable")
        .args(["--json", "match", "cask:tool"])
        .output()
        .unwrap();
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{reply}");
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["data"]["errors"], serde_json::json!([]));
    assert_eq!(reply["data"]["repositories"][0]["repo"], "acme/tool");
    s.unchanged();
}

#[test]
fn broken_loose_candidates_are_notes_and_substring_noise_is_never_read() {
    // Given a valid fd package, a broken prefix candidate and an unrelated substring hit.
    let s = sandbox();
    s.tool("brew", r#"
case "$*" in
'info --installed --json=v2') echo '{}' ;;
'search --formula fd') printf 'fd\nfd-helper\nfrankieew/tap/lmfdb-cli\n' ;;
'search --cask fd') exit 0 ;;
'info --json=v2 --formula fd') echo '{"formulae":[{"name":"fd","homepage":"https://github.com/sharkdp/fd"}]}' ;;
*lmfdb*) echo "$*" >> "$HEADWAY_HOME/noise-read"; exit 1 ;;
*) exit 1 ;;
esac
"#);
    // When matching, then skipped loose candidates leave the machine result successful.
    let reply = s.ok(&["match", "sharkdp/fd", "--offline"]);
    assert_eq!(reply["data"]["errors"], serde_json::json!([]));
    assert_eq!(
        reply["data"]["notes"],
        serde_json::json!([
            "Skipped Homebrew formula `fd-helper`: its metadata could not be read."
        ])
    );
    assert_eq!(reply["data"]["packages"][0]["package"], "fd");
    assert!(!s.dir.path().join("noise-read").exists());
    s.unchanged();
}
