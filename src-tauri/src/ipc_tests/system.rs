//! System commands: link policy and the host adapter boundary.

use serde_json::json;

use super::host::HostCall;
use super::support::{call, harness, ok};

#[test]
fn absolute_http_links_reach_the_host_and_nothing_else_does() {
    let harness = harness("http://127.0.0.1:9");
    let accepted = [
        "https://github.com/octo/tool",
        "http://example.invalid/path?q=1#frag",
        "https://user@example.invalid/x",
        "https://[::1]:8080/x",
    ];
    for url in accepted {
        ok(&harness, "open_url", json!({ "url": url }));
    }
    assert_eq!(
        harness.host.recorded(),
        accepted
            .iter()
            .map(|url| HostCall::OpenUrl((*url).to_owned()))
            .collect::<Vec<_>>()
    );

    // Not GitHub-only: other sources link to their own pages, but only over
    // http(s) — these are all refused before the system opener sees them.
    let refused = [
        "file:///etc/passwd",
        "smb://server/share",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "ftp://example.invalid/x",
        "/Applications/Tool.app",
        "https://",
        "https://exa mple.invalid/",
        "https://example.invalid/\n--flag",
        "",
    ];
    for url in refused {
        let error = call(&harness, "open_url", json!({ "url": url })).unwrap_err();
        assert_eq!(error["code"], "invalid_input", "accepted `{url}`");
    }
    assert_eq!(
        harness.host.recorded().len(),
        accepted.len(),
        "rejected links must never reach the opener"
    );
}

#[test]
fn reveal_requires_an_existing_path_and_uses_the_host() {
    let harness = harness("http://127.0.0.1:9");
    let folder = harness.home.path().join("logs");
    std::fs::create_dir_all(&folder).unwrap();
    ok(
        &harness,
        "reveal_path",
        json!({ "path": folder.display().to_string() }),
    );
    assert_eq!(
        harness.host.recorded(),
        vec![HostCall::Reveal(folder.display().to_string())]
    );

    let missing = harness.home.path().join("missing");
    let error = call(
        &harness,
        "reveal_path",
        json!({ "path": missing.display().to_string() }),
    )
    .unwrap_err();
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(harness.host.recorded().len(), 1);
}

#[test]
fn autostart_reads_and_writes_through_the_host_adapter() {
    let harness = harness("http://127.0.0.1:9");
    assert_eq!(ok(&harness, "get_autostart", json!({})), false);
    assert_eq!(
        ok(&harness, "set_autostart", json!({ "enabled": true })),
        true
    );
    assert!(harness.host.autostart());
    assert_eq!(ok(&harness, "get_autostart", json!({})), true);
    assert_eq!(
        ok(&harness, "set_autostart", json!({ "enabled": false })),
        false
    );
    assert!(!harness.host.autostart());
}
