use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::token::SecretStore;
use crate::{Headway, Source, TrackedApp};
use std::sync::Arc;

struct UnavailableSecrets;
impl SecretStore for UnavailableSecrets {
    fn get(&self) -> crate::Result<Option<String>> {
        panic!("inventory must not resolve a token")
    }
    fn set(&self, _: &str) -> crate::Result<()> {
        panic!("must not write secrets")
    }
    fn delete(&self) -> crate::Result<bool> {
        panic!("must not delete secrets")
    }
}

fn headway(env: ShellEnv) -> (tempfile::TempDir, Headway) {
    let dir = tempfile::tempdir().unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(dir.path()))
        .env(env)
        .secrets(Arc::new(UnavailableSecrets))
        .build()
        .unwrap();
    (dir, hw)
}
fn app(id: &str, package: &str) -> TrackedApp {
    let mut app = crate::recipes::recipes().remove(2).app;
    app.id = id.into();
    app.name = package.into();
    app.source = Source::Npm(crate::NpmSource {
        package: package.into(),
        dist_tag: "latest".into(),
    });
    app
}

#[test]
fn imports_selected_payloads_with_auto_update_disabled() {
    // Given a user-edited selection with auto-update enabled.
    let (_dir, hw) = headway(ShellEnv::default());
    let mut selected = app("selected", "pnpm");
    selected.auto_update = true;
    // When importing.
    let views = hw.import_apps(vec![selected]).unwrap();
    // Then the payload is persisted with safe defaults and no install state.
    assert_eq!(views.len(), 1);
    assert!(!views[0].app.auto_update);
    assert!(views[0].installed.is_none());
    assert_eq!(
        hw.store().config().unwrap().apps,
        vec![views[0].app.clone()]
    );
    assert!(!hw.paths().state_file().exists());
}

#[test]
fn duplicate_or_invalid_batches_never_partially_write_config() {
    // Given an existing config and batches whose later item is rejected.
    let (_dir, hw) = headway(ShellEnv::default());
    hw.import_apps(vec![app("existing", "pnpm")]).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    let batches = [
        vec![app("new", "jq"), app("existing", "other")],
        vec![app("new", "jq"), app("another", "pnpm")],
        vec![app("new", "jq"), app("new", "other")],
        vec![app("new", "jq"), app("another", "jq")],
        vec![app("new", "jq"), app("invalid/id", "other")],
    ];
    for batch in batches {
        // When importing a rejected batch.
        assert!(hw.import_apps(batch).is_err());
        // Then the config bytes are exactly unchanged.
        assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
    }
}

#[tokio::test]
async fn discovery_flags_existing_identity_and_conflicting_names_without_writes() {
    // Given an installed package tracked under another ID and two discovered packages sharing a name.
    let tools = crate::testing::FakeTools::new()
        .tool("npm", r#"echo '{"dependencies":{"pnpm":{"version":"10.0.0"}}}'"#)
        .tool("brew", r#"echo '{"formulae":[{"name":"pnpm","installed":[{"version":"10.0.0"}]}],"casks":[]}'"#);
    let (_dir, hw) = headway(tools.env());
    hw.import_apps(vec![app("custom-id", "pnpm")]).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    // When discovering through the public API with an unusable secret store.
    let report = hw.discover_apps(&[]).await.unwrap();
    // Then identity and name conflicts surface without modifying config or state.
    assert!(report.errors.is_empty());
    assert_eq!(report.candidates.len(), 2);
    assert_eq!(
        report
            .candidates
            .iter()
            .filter(|item| item.already_tracked)
            .count(),
        1
    );
    assert!(
        report
            .candidates
            .iter()
            .all(|item| !item.conflicts.is_empty())
    );
    assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
    assert!(!hw.paths().state_file().exists());
}

#[test]
fn import_planning_rejects_conflicts_without_writes_and_matches_import_errors() {
    // Given an existing tracked source and invalid batches with valid first entries.
    let (_dir, hw) = headway(ShellEnv::default());
    hw.import_apps(vec![app("existing", "pnpm")]).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    let batches = [
        vec![app("new", "jq"), app("another", "pnpm")],
        vec![app("new", "jq"), app("existing", "other")],
        vec![app("new", "jq"), app("new", "other")],
        vec![app("new", "jq"), app("another", "jq")],
        vec![app("new", "jq"), app("invalid/id", "other")],
    ];
    for batch in batches {
        // When planning and executing the same invalid batch.
        let planned = hw.plan_import_apps(batch.clone()).unwrap_err();
        let imported = hw.import_apps(batch).unwrap_err();
        // Then both return the same error and leave config bytes and state untouched.
        assert_eq!(
            crate::ErrorInfo::from(&planned),
            crate::ErrorInfo::from(&imported)
        );
        assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
        assert!(!hw.paths().state_file().exists());
    }
}

#[test]
fn import_planning_returns_safe_payloads_without_creating_store_files() {
    // Given an empty store and a selection requesting automatic updates.
    let (dir, hw) = headway(ShellEnv::default());
    let mut selected = app("selected", "pnpm");
    selected.auto_update = true;
    // When planning a valid import.
    let planned = hw.plan_import_apps(vec![selected.clone()]).unwrap();
    // Then only auto-update is disabled, and not even the store lock was created.
    selected.auto_update = false;
    assert_eq!(planned, vec![selected]);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn importing_a_stale_plan_rejects_newly_tracked_identity() {
    // Given a plan followed by a competing import of its source under another ID.
    let (_dir, hw) = headway(ShellEnv::default());
    let planned = hw.plan_import_apps(vec![app("planned", "pnpm")]).unwrap();
    hw.import_apps(vec![app("competing", "pnpm")]).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    // When executing the stale plan.
    let result = hw.import_apps(planned);
    // Then the new conflict is rejected without changing the competing entry.
    assert_eq!(result.unwrap_err().code(), "invalid_input");
    assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
}

#[test]
fn import_rejects_github_case_and_url_aliases_without_writing() {
    // Given an existing GitHub subscription using the casing from the installed config.
    let (_dir, hw) = headway(ShellEnv::default());
    let mut original = crate::recipes::recipes().remove(3).app;
    if let Source::Github(github) = &mut original.source {
        github.repo = "Frankieew/headway".into();
    }
    hw.import_apps(vec![original.clone()]).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    for repo in [
        "FrankieeW/Headway",
        "https://github.com/FRANKIEEW/HEADWAY.git",
    ] {
        let mut duplicate = original.clone();
        duplicate.id = "alias".into();
        if let Source::Github(github) = &mut duplicate.source {
            github.repo = repo.into();
        }
        // When planning or importing the same repository through another spelling.
        let planned = hw.plan_import_apps(vec![duplicate.clone()]);
        let imported = hw.import_apps(vec![duplicate]);
        // Then the alternate spelling cannot create a duplicate writer.
        assert_eq!(planned.unwrap_err().code(), "invalid_input");
        assert_eq!(imported.unwrap_err().code(), "invalid_input");
        assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
    }
}
