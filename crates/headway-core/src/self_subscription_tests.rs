use std::os::unix::fs::PermissionsExt as _;

use super::*;

#[expect(
    clippy::unwrap_used,
    reason = "isolated test fixture setup must fail loudly"
)]
fn fixture(dirty: bool) -> (tempfile::TempDir, Headway) {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("apps/Headway.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>wang.frankie.headway</string><key>CFBundleExecutable</key><string>headway-desktop</string></dict></plist>"#).unwrap();
    let reply = serde_json::json!({"ok":true,"data":{"version":"0.2.1","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","dirty":dirty,"target":"aarch64-apple-darwin"}});
    for name in ["headway", "headway-desktop"] {
        let path = app.join("Contents/MacOS").join(name);
        std::fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' '{reply}'\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let hw = Headway::builder()
        .paths(crate::paths::Paths::under(root.path().join("home")))
        .build()
        .unwrap();
    hw.set_setting("apps_dir", root.path().join("apps").to_str().unwrap())
        .unwrap();
    (root, hw)
}

#[tokio::test]
async fn a_fork_at_the_actual_bundle_is_preserved_without_claiming_an_official_sha() {
    let (root, hw) = fixture(false);
    hw.add(crate::testing::cli_app("other")).unwrap();
    let mut app = crate::testing::cli_app("my-headway-fork");
    app.kind = AppKind::App;
    let Source::Github(github) = &mut app.source else {
        unreachable!()
    };
    github.repo = "my-fork/headway".into();
    let Install::Source(source) = &mut github.install else {
        unreachable!()
    };
    source.artifact = Some("Headway.app".into());
    source.dest = Some(root.path().join("apps/Headway.app").display().to_string());
    hw.add(app.clone()).unwrap();
    let current = hw.ensure_self_subscription().await.unwrap();
    assert_eq!(current.app, app);
    assert!(current.installed.is_none());
    assert_eq!(hw.list().unwrap().len(), 2);
    assert_eq!(hw.list().unwrap()[0].app.id, "my-headway-fork");
}

#[tokio::test]
async fn independent_running_versions_do_not_register_another_primary_subscription() {
    let (_root, hw) = fixture(false);
    let mut app = crate::testing::cli_app("independent");
    let Source::Github(github) = &mut app.source else {
        unreachable!()
    };
    github.options.variant = Some(crate::VariantInfo {
        parent_id: "primary".into(),
        reference: "a".repeat(40),
        release_tag: None,
        branch: Some("main".into()),
    });
    let Install::Source(source) = &mut github.install else {
        unreachable!()
    };
    source.dest = Some(std::env::current_exe().unwrap().display().to_string());
    hw.add(app.clone()).unwrap();
    let before = std::fs::read(hw.paths().config_file()).unwrap();
    let current = hw.ensure_self_subscription().await.unwrap();
    assert_eq!(current.app, app);
    assert_eq!(hw.list().unwrap().len(), 1);
    assert_eq!(std::fs::read(hw.paths().config_file()).unwrap(), before);
}

#[tokio::test]
async fn self_is_first_without_removing_existing_apps_and_only_observed_commit_is_adopted() {
    // Given another existing subscription and a clean stamped installed desktop app.
    let (_root, hw) = fixture(false);
    let other = hw.add(crate::testing::cli_app("other")).unwrap().app;
    // When adding self twice, as startup and a CLI request might do.
    hw.ensure_self_subscription().await.unwrap();
    let view = hw.ensure_self_subscription().await.unwrap();
    // Then self is first, idempotent, notification-only, with an observed baseline.
    let apps = hw.list().unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[0].app.id, view.app.id);
    assert_eq!(apps[1].app, other);
    assert!(!view.app.auto_update);
    assert_eq!(view.app.update_policy.mode, AutoUpdateMode::Notify);
    assert_eq!(
        view.installed.unwrap().id,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert!(hw.history(&view.app.id).unwrap().is_empty());
}

#[tokio::test]
async fn existing_self_configuration_is_preserved_while_order_changes() {
    // Given an existing self subscription with a custom build and chosen update policy.
    let (_root, hw) = fixture(false);
    hw.add(crate::testing::cli_app("other")).unwrap();
    let mut app = crate::recipes::recipes()
        .into_iter()
        .find(|recipe| recipe.id == "headway-app")
        .unwrap()
        .app;
    app.id = "my-headway".into();
    app.auto_update = true;
    app.update_policy.mode = AutoUpdateMode::Next;
    let Source::Github(github) = &mut app.source else {
        panic!("GitHub recipe");
    };
    let Install::Source(source) = &mut github.install else {
        panic!("source recipe");
    };
    source.build = "make my-custom-app".into();
    hw.add(app.clone()).unwrap();
    // When startup ensures the first self subscription.
    let view = hw.ensure_self_subscription().await.unwrap();
    // Then only ordering and observed state change, not the user's configuration.
    assert_eq!(view.app, app);
    assert_eq!(hw.list().unwrap()[0].app.id, "my-headway");
}

#[tokio::test]
async fn dirty_local_build_never_becomes_a_clean_remote_baseline() {
    // Given local source changes in the installed app.
    let (_root, hw) = fixture(true);
    // When self is subscribed.
    let view = hw.ensure_self_subscription().await.unwrap();
    // Then it remains tracked without inventing a clean installed commit.
    assert!(view.installed.is_none());
    assert!(!view.app.auto_update);
}

#[tokio::test]
async fn release_and_tag_subscription_baselines_remain_tags_after_startup() {
    use serde_json::json;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};
    for track in [
        Track::Release {
            prerelease: false,
            tag_filter: None,
        },
        Track::Tag {
            prerelease: false,
            tag_filter: None,
        },
    ] {
        // Given an existing release/tag baseline and the same stamped desktop code.
        let (_root, hw) = fixture(false);
        let mut app = crate::recipes::recipes()
            .into_iter()
            .find(|recipe| recipe.id == "headway-app")
            .unwrap()
            .app;
        let Source::Github(github) = &mut app.source else {
            panic!("GitHub recipe");
        };
        github.track = track.clone();
        hw.add(app.clone()).unwrap();
        hw.store()
            .update_state(|state| {
                Ok((
                    state.with_app(&app.id, |state| crate::state::AppState {
                        installed: Some(InstalledRef {
                            id: "v0.2.1".into(),
                            sha: Some("a".repeat(40)),
                            version: Some("v0.2.1".into()),
                            installed_at: Utc::now(),
                            path: Some(hw.paths().config_dir.display().to_string()),
                        }),
                        ..state
                    }),
                    (),
                ))
            })
            .unwrap();
        // When startup adopts self ordering.
        let view = hw.ensure_self_subscription().await.unwrap();
        // Then the release identity remains valid for the normal version checker.
        let installed = hw.store().state().unwrap().app(&app.id).installed.unwrap();
        assert_eq!(installed.id, "v0.2.1");
        let server = MockServer::start().await;
        let (endpoint, body) = match track {
            Track::Release { .. } => (
                "releases",
                json!([{"id":1,"tag_name":"v0.2.1","assets":[],"draft":false,"prerelease":false}]),
            ),
            Track::Tag { .. } => (
                "tags",
                json!([{"name":"v0.2.1","commit":{"sha":"a".repeat(40)}}]),
            ),
            Track::Commit { .. } => panic!("version mode"),
        };
        Mock::given(path(format!("/repos/FrankieeW/headway/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let client = crate::github::GitHub::new(&server.uri(), None).unwrap();
        let check =
            crate::check::check_github(&client, view.app.github().unwrap(), Some(&installed))
                .await
                .unwrap();
        assert_eq!(check.status, crate::UpdateStatus::UpToDate);
    }
}

#[tokio::test]
async fn a_different_custom_destination_never_adopts_the_discovered_bundle_revision() {
    // Given another installed copy that the existing subscription explicitly targets.
    let (root, hw) = fixture(false);
    let other = root.path().join("custom/Headway.app");
    std::fs::create_dir_all(&other).unwrap();
    let mut app = crate::recipes::recipes()
        .into_iter()
        .find(|recipe| recipe.id == "headway-app")
        .unwrap()
        .app;
    let Source::Github(github) = &mut app.source else {
        panic!("GitHub recipe");
    };
    let Install::Source(source) = &mut github.install else {
        panic!("source recipe");
    };
    source.dest = Some(other.display().to_string());
    hw.add(app.clone()).unwrap();
    let previous = InstalledRef {
        id: "b".repeat(40),
        sha: Some("b".repeat(40)),
        version: Some("0.1.0".into()),
        installed_at: Utc::now(),
        path: Some(other.display().to_string()),
    };
    hw.store()
        .update_state(|state| {
            Ok((
                state.with_app(&app.id, |state| crate::state::AppState {
                    installed: Some(previous.clone()),
                    ..state
                }),
                (),
            ))
        })
        .unwrap();
    // When self ordering observes a separate apps/Headway.app at commit a...a.
    hw.ensure_self_subscription().await.unwrap();
    // Then neither the target nor its revision is transplanted from that other copy.
    assert_eq!(hw.get(&app.id).unwrap().app, app);
    assert_eq!(
        hw.store().state().unwrap().app(&app.id).installed,
        Some(previous)
    );
}

#[tokio::test]
async fn an_active_update_prevents_observation_from_overwriting_its_state() {
    let (_root, hw) = fixture(false);
    let view = hw.ensure_self_subscription().await.unwrap();
    let before = hw.store().state().unwrap().app(&view.app.id);
    let _lock =
        crate::lock::AppLock::acquire(&hw.paths().app_lock(&view.app.id), &view.app.id).unwrap();
    let error = hw.ensure_self_subscription().await.unwrap_err();
    assert_eq!(error.code(), "busy");
    assert_eq!(hw.store().state().unwrap().app(&view.app.id), before);
}
