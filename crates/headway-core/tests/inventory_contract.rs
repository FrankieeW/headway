//! Public API contract exercised with isolated executable fixtures and stores.
#![expect(
    clippy::unwrap_used,
    reason = "Integration-test fixtures and assertions must fail immediately on unexpected errors"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;

use headway_core::paths::Paths;
use headway_core::shell::ShellEnv;
use headway_core::token::SecretStore;
use headway_core::{Headway, Result, Source};

struct NoSecrets;
#[expect(
    clippy::panic,
    reason = "The test secret store must fail loudly if inventory accesses credentials"
)]
impl SecretStore for NoSecrets {
    fn get(&self) -> Result<Option<String>> {
        panic!("token lookup")
    }
    fn set(&self, _: &str) -> Result<()> {
        panic!("token write")
    }
    fn delete(&self) -> Result<bool> {
        panic!("token delete")
    }
}
fn artifact(name: &str, value: &impl serde::Serialize) {
    if let Ok(directory) = std::env::var("HEADWAY_INVENTORY_EVIDENCE") {
        std::fs::write(
            std::path::Path::new(&directory).join(name),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
}
fn tool(root: &std::path::Path, name: &str, body: &str) {
    let path = root.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn inventory_import_and_recipes_work_through_public_api() {
    // Given isolated tools that accept only inventory commands and reject token leakage.
    let root = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    tool(
        bin.path(),
        "brew",
        r#"
[ "$*" = 'info --installed --json=v2' ] || exit 90
[ "$HOMEBREW_NO_AUTO_UPDATE" = 1 ] || exit 91
[ -z "$HEADWAY_GITHUB_TOKEN" ] || exit 92
echo '{"formulae":[{"name":"tool","full_name":"acme/tap/tool","linked_keg":"2.0","installed":[{"version":"1.0"},{"version":"2.0"}]}],"casks":[{"token":"editor","tap":"acme/apps","name":["Editor"],"installed":"3.0,42"}]}'
"#,
    );
    tool(
        bin.path(),
        "npm",
        r#"
[ "$*" = 'ls --global --depth=0 --json' ] || exit 90
[ -z "$HEADWAY_GITHUB_TOKEN" ] || exit 92
echo '{"dependencies":{"@acme/tool":{"version":"1.2.3"},"acme-tool":{"version":"2.0.0"}}}'
exit 1
"#,
    );
    let env = ShellEnv::from_vars([])
        .with_var("PATH", bin.path().display().to_string())
        .with_var("HEADWAY_GITHUB_TOKEN", "test-secret");
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .env(env)
        .secrets(Arc::new(NoSecrets))
        .build()
        .unwrap();
    // When discovering, importing selected payloads, then rediscovering.
    let report = hw.discover_apps(&[]).await.unwrap();
    artifact("discovery.json", &report);
    assert_eq!(report.candidates.len(), 4);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].error.code, "command_failed");
    assert!(!hw.paths().config_file().exists());
    let mut apps: Vec<_> = report
        .candidates
        .iter()
        .map(|candidate| candidate.app.clone())
        .collect();
    for app in &mut apps {
        app.auto_update = true;
        app.validate().unwrap();
    }
    let views = hw.import_apps(apps).unwrap();
    artifact("import.json", &views);
    let rediscovered = hw.discover_apps(&[]).await.unwrap();
    artifact("rediscovery.json", &rediscovered);
    // Then the real config contains only safe selections, without any install or update state.
    assert!(
        views
            .iter()
            .all(|view| !view.app.auto_update && view.installed.is_none())
    );
    assert!(
        rediscovered
            .candidates
            .iter()
            .all(|candidate| candidate.already_tracked)
    );
    assert!(!hw.paths().state_file().exists());
    assert_eq!(
        report
            .candidates
            .iter()
            .map(|item| &item.app.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        4
    );
    assert!(
        report
            .candidates
            .iter()
            .any(|item| item.installed_version.as_deref() == Some("2.0")
                && !item.conflicts.is_empty())
    );
    assert!(report.candidates.iter().any(|item| matches!(&item.app.source, Source::Brew(brew) if brew.cask && brew.package == "acme/apps/editor")));

    // Given valid first items followed by conflicting identities, IDs, or invalid payloads.
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    let fresh = hw.recipes()[0].app.clone();
    let mut duplicate = views[0].app.clone();
    duplicate.id = "renamed".into();
    let mut invalid = fresh.clone();
    invalid.id = "bad/id".into();
    let batches = [
        vec![fresh.clone(), duplicate],
        vec![fresh.clone(), views[0].app.clone()],
        vec![fresh.clone(), fresh.clone()],
        vec![fresh.clone(), invalid],
    ];
    let mut errors = Vec::new();
    for batch in batches {
        // When attempting atomic import.
        errors.push(headway_core::ErrorInfo::from(
            &hw.import_apps(batch).unwrap_err(),
        ));
        // Then no first item was persisted.
        assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
    }
    artifact("rejected-imports.json", &errors);
    let recipes = hw.recipes();
    artifact("recipes.json", &recipes);
    for recipe in recipes {
        recipe.app.validate().unwrap();
        assert!(!recipe.app.auto_update);
    }
}

#[tokio::test]
async fn missing_tools_return_errors_without_touching_config() {
    // Given a PATH without executables.
    let root = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .env(ShellEnv::from_vars([]).with_var("PATH", "/headway-no-tools"))
        .secrets(Arc::new(NoSecrets))
        .build()
        .unwrap();
    // When requesting the default sources.
    let report = hw.discover_apps(&[]).await.unwrap();
    artifact("missing-tools.json", &report);
    // Then both failures are structured and discovery is read-only.
    assert!(report.candidates.is_empty());
    assert_eq!(report.errors.len(), 2);
    assert!(
        report
            .errors
            .iter()
            .all(|error| error.error.code == "io_error")
    );
    assert!(!hw.paths().config_file().exists());
}
