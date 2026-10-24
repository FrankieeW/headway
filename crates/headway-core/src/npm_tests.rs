use super::*;
use crate::testing::FakeTools;

const VIEW: &str = r#"{
        "versions": ["1.0.0", "1.1.0", "1.2.0-beta.1", "1.2.0", "2.0.0-rc.1"],
        "dist-tags": {"latest": "1.2.0", "next": "2.0.0-rc.1"},
        "time": {"1.1.0": "2026-09-01T10:00:00.000Z", "1.2.0": "2026-10-01T10:00:00.000Z"},
        "description": "A test tool"
    }"#;

#[tokio::test]
async fn next_version_remains_adjacent_when_pending_versions_are_truncated() {
    // Given more published upgrades than fit in the pending window.
    let versions: Vec<String> = (0..=150).map(|minor| format!("1.{minor}.0")).collect();
    let view = serde_json::json!({"versions": versions, "dist-tags": {"latest": "1.150.0"}});
    let tools = FakeTools::new().tool("npm", &format!("case \"$1\" in view) echo '{view}';; ls) echo '{{\"dependencies\":{{\"@acme/tool\":{{\"version\":\"1.0.0\"}}}}}}';; esac"));
    // When checking for available versions.
    let checked = check(&CancellationToken::new(), &tools.env(), &source("latest"))
        .await
        .unwrap();
    // Then stepping picks the immediate successor, while the count stays complete.
    assert_eq!(checked.result.next().unwrap().id, "1.1.0");
    assert_eq!(checked.result.pending_total, 150);
    assert_eq!(checked.result.pending.len(), PENDING_LIMIT);
}

fn source(tag: &str) -> NpmSource {
    NpmSource {
        package: "@acme/tool".into(),
        dist_tag: tag.into(),
    }
}

fn tools(installed: Option<&str>) -> FakeTools {
    let listing = installed.map_or_else(
        || "{}".to_owned(),
        |version| format!(r#"{{"dependencies":{{"@acme/tool":{{"version":"{version}"}}}}}}"#),
    );
    FakeTools::new().tool(
            "npm",
            &format!(
                "case \"$1\" in view) echo '{}';; ls) echo '{listing}';; install) echo \"installed $3\";; esac",
                VIEW.replace('\n', " ")
            ),
        )
}

#[tokio::test]
async fn stable_channel_lists_newer_stable_versions() {
    let tools = tools(Some("1.0.0"));
    let checked = check(&CancellationToken::new(), &tools.env(), &source("latest"))
        .await
        .unwrap();
    let result = checked.result;
    assert_eq!(result.status, UpdateStatus::UpdateAvailable);
    let ids: Vec<&str> = result.pending.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["1.2.0", "1.1.0"], "no pre-releases on `latest`");
    assert_eq!(result.latest.unwrap().id, "1.2.0");
    assert!(result.pending[0].date.is_some());
    assert_eq!(
        checked.installed,
        InstalledSync::Detected(Some("1.0.0".into()))
    );
}

#[tokio::test]
async fn next_channel_includes_prereleases_and_missing_installs_are_reported() {
    let tools = tools(None);
    let checked = check(&CancellationToken::new(), &tools.env(), &source("next"))
        .await
        .unwrap();
    assert_eq!(checked.result.status, UpdateStatus::NotInstalled);
    assert_eq!(checked.result.latest.unwrap().id, "2.0.0-rc.1");
    assert_eq!(checked.installed, InstalledSync::Detected(None));

    let current = tools_up_to_date();
    let checked = check(&CancellationToken::new(), &current.env(), &source("latest"))
        .await
        .unwrap();
    assert_eq!(checked.result.status, UpdateStatus::UpToDate);
}

fn tools_up_to_date() -> FakeTools {
    tools(Some("1.2.0"))
}

#[tokio::test]
async fn explicit_versions_must_exist_and_lookups_suggest_an_app() {
    let tools = tools(None);
    let env = tools.env();
    assert!(
        version(&CancellationToken::new(), &env, &source("latest"), "1.1.0")
            .await
            .is_ok()
    );
    assert!(matches!(
        version(&CancellationToken::new(), &env, &source("latest"), "9.9.9").await,
        Err(Error::RemoteNotFound(_))
    ));
    let found = lookup(&CancellationToken::new(), &env, "@acme/tool")
        .await
        .unwrap();
    assert_eq!(found.suggestion.id, "tool");
    assert_eq!(found.latest.as_deref(), Some("1.2.0"));
    assert_eq!(found.description.as_deref(), Some("A test tool"));
    assert!(found.notes[1].contains("Not installed"));
}

#[tokio::test]
async fn unknown_packages_are_not_found() {
    let tools = FakeTools::new().tool("npm", "echo '{\"error\":{\"code\":\"E404\"}}'; exit 1");
    let err = check(&CancellationToken::new(), &tools.env(), &source("latest"))
        .await
        .unwrap_err();
    assert!(matches!(&err, Error::RemoteNotFound(msg) if msg.contains("@acme/tool")));
}
