use super::*;
use crate::testing::FakeTools;

#[test]
fn parses_multiple_kegs_and_tap_casks_when_brew_reports_installs() {
    // Given realistic installed metadata, including multiple formula kegs.
    let json = r#"{"formulae":[{"name":"tool","full_name":"acme/tap/tool","linked_keg":"2.0_1","installed":[{"version":"1.0"},{"version":"2.0_1"}]},{"name":"absent","installed":[]}],"casks":[{"token":"editor","full_token":"acme/apps/editor","name":["Editor"],"installed":"3.2,42"},{"token":"gone","installed":null}]}"#;
    // When parsing the package manager's output.
    let items = parse::brew(json).unwrap();
    // Then installed versions and full source identities survive.
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].installed_version.as_deref(), Some("2.0_1"));
    assert_eq!(items[0].conflicts.len(), 1);
    assert!(
        matches!(&items[0].app.source, Source::Brew(s) if s.package == "acme/tap/tool" && !s.cask)
    );
    assert!(
        matches!(&items[1].app.source, Source::Brew(s) if s.package == "acme/apps/editor" && s.cask)
    );
    assert_eq!(items[1].installed_version.as_deref(), Some("3.2,42"));
}

#[test]
fn ids_are_distinct_safe_and_stable_when_names_collapse_or_are_long() {
    // Given scoped and unscoped names with the same readable slug.
    let long = format!("@scope/{}", "x".repeat(180));
    let input = serde_json::json!({"dependencies": {"@a/b": {"version": "1.0.0"}, "a-b": {"version": "2.0.0"}, long: {"version": "3.0.0"}, "missing": {"missing": true}}}).to_string();
    // When parsing twice.
    let first = parse::npm(&input).unwrap();
    let second = parse::npm(&input).unwrap();
    // Then IDs are valid, stable, unique and never enable auto-update.
    let ids: Vec<_> = first.iter().map(|item| &item.app.id).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(
        ids,
        second.iter().map(|item| &item.app.id).collect::<Vec<_>>()
    );
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3
    );
    for item in first {
        item.app.validate().unwrap();
        assert!(!item.app.auto_update);
    }
}

#[tokio::test]
async fn missing_tools_are_partial_errors_when_path_is_empty() {
    // Given a per-command PATH that contains no executables.
    let env = ShellEnv::from_vars([]).with_var("PATH", "/headway-test-no-tools");
    // When both sources are discovered.
    let report = discover(&[], &env).await.unwrap();
    // Then each failure is reported, without failing the report itself.
    assert!(report.candidates.is_empty());
    assert_eq!(report.errors.len(), 2);
    assert!(
        report
            .errors
            .iter()
            .all(|error| error.error.code == "io_error")
    );
}

#[tokio::test]
async fn retains_usable_npm_entries_when_command_exits_nonzero() {
    // Given executable fixtures that reject every command except the read-only inventory query.
    let tools = FakeTools::new()
        .tool(
            "npm",
            r#"
[ "$*" = 'ls --global --depth=0 --json' ] || exit 99
[ -z "$HEADWAY_GITHUB_TOKEN" ] || exit 98
printf '%s\n' '{"dependencies":{"@acme/tool":{"version":"1.2.3"}},"problems":["extraneous"]}'
exit 1
"#,
        )
        .tool(
            "brew",
            r#"
[ "$*" = 'info --installed --json=v2' ] || exit 99
[ "$HOMEBREW_NO_AUTO_UPDATE" = 1 ] || exit 98
printf '%s\n' '{"formulae":[],"casks":[]}'
"#,
        );
    // When discovery runs with a token present in its input environment.
    let report = discover(
        &[],
        &tools.env().with_var("HEADWAY_GITHUB_TOKEN", "test-secret"),
    )
    .await
    .unwrap();
    // Then useful packages and the command error coexist.
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].source, "npm");
    assert_eq!(report.errors[0].error.code, "command_failed");
    assert_eq!(
        report.candidates[0].installed_version.as_deref(),
        Some("1.2.3")
    );
}

#[tokio::test]
async fn rejects_unsupported_sources_before_running_tools() {
    // Given no tools on PATH and one unsupported source.
    let env = ShellEnv::from_vars([]).with_var("PATH", "/headway-test-no-tools");
    // When requesting mixed supported and unsupported sources.
    let result = discover(&["npm".into(), "script".into()], &env).await;
    // Then validation fails before subprocess discovery.
    assert_eq!(result.unwrap_err().code(), "invalid_input");
}

#[test]
fn malformed_output_is_an_error_when_not_inventory_json() {
    // Given malformed metadata.
    // When parsing each source.
    let results = [parse::brew("{}"), parse::npm("not json")];
    // Then it is never silently treated as empty inventory.
    assert!(results.iter().all(Result::is_err));
}

#[tokio::test]
async fn repeated_sources_and_duplicate_entries_get_unique_ids_and_conflicts() {
    // Given duplicate rows in a package-manager response.
    let tools = FakeTools::new().tool("brew", r#"echo '{"formulae":[{"name":"tool","installed":[{"version":"1"}]},{"name":"tool","installed":[{"version":"1"}]}],"casks":[]}'"#);
    let root = tempfile::tempdir().unwrap();
    let hw = crate::Headway::builder()
        .paths(crate::paths::Paths::under(root.path()))
        .env(tools.env())
        .build()
        .unwrap();
    // When the same inventory source is requested twice.
    let report = hw
        .discover_apps(&["brew".into(), "brew".into()])
        .await
        .unwrap();
    // Then the query is deduplicated but duplicate source writers are visible.
    assert_eq!(report.candidates.len(), 2);
    assert_ne!(report.candidates[0].app.id, report.candidates[1].app.id);
    assert!(
        report
            .candidates
            .iter()
            .all(|item| !item.conflicts.is_empty())
    );
    assert!(!hw.paths().config_file().exists());
}

#[test]
fn ambiguous_kegs_are_not_guessed_and_cask_taps_are_preserved() {
    // Given unlinked multiple kegs and a third-party cask without full_token.
    let input = r#"{"formulae":[{"name":"tool","installed":[{"version":"1"},{"version":"2"}]}],"casks":[{"token":"editor","tap":"acme/apps","installed":"3"}]}"#;
    // When parsing.
    let items = parse::brew(input).unwrap();
    // Then no active version is invented and the cask keeps its owning tap.
    assert!(items[0].installed_version.is_none());
    assert_eq!(items[1].key, "brew cask acme/apps/editor");
}
