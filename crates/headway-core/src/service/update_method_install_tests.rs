use super::*;
use crate::{AssetInstall, Target, UpdateRequest};

#[tokio::test]
async fn forced_asset_replaces_the_saved_destination_and_keeps_the_recipe() {
    // Given a source installation with a different destination in its alternate recipe.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    f.release(true).await;
    let alternate = f.dir.path().join("other/tool");
    let app = with_github(f.hw.get("tool").unwrap().app, |mut source| {
        source.options.release_asset = Some(AssetInstall {
            dest: Some(alternate.display().to_string()),
            ..Default::default()
        });
        source
    });
    f.hw.replace(app.clone()).unwrap();
    let request = UpdateRequest {
        target: Target::Ref(f.new.clone()),
        method: RevisionMethod::Asset,
    };
    let plan = f.hw.plan_with("tool", &request).await.unwrap();
    assert_eq!(plan.method, "asset");
    assert!(
        plan.steps
            .iter()
            .any(|step| step.contains(&f.dest.display().to_string()))
    );
    assert!(
        !plan
            .steps
            .iter()
            .any(|step| step.contains(&alternate.display().to_string()))
    );

    // When the public update API downloads and installs the selected commit's release.
    f.hw.update_with("tool", &request, &NullSink, &CancellationToken::new())
        .await
        .unwrap();

    // Then execution agrees with the plan and ordinary recovery accepts the result.
    crate::recovery::reconcile(f.hw.paths(), &app).unwrap();
    f.assert_payload(&f.new, "download");
    assert!(!alternate.exists());
    assert_eq!(f.hw.get("tool").unwrap().app, app);
    assert!(!crate::recovery::pending(f.hw.paths(), &app));
    f.capture("forced-asset");
}

#[tokio::test]
async fn forced_source_uses_the_asset_destination_and_keeps_the_recipe() {
    // Given an asset recipe with an alternate source build pointing elsewhere.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    let alternate = f.dir.path().join("other/tool");
    let app = with_github(f.hw.get("tool").unwrap().app, |mut source| {
        let Install::Source(mut build) = source.install else {
            unreachable!()
        };
        build.dest = Some(alternate.display().to_string());
        source.options.source_build = Some(build);
        source.install = Install::Asset(AssetInstall {
            dest: Some(f.dest.display().to_string()),
            ..Default::default()
        });
        source
    });
    f.hw.replace(app.clone()).unwrap();
    let request = UpdateRequest {
        target: Target::Ref(f.new.clone()),
        method: RevisionMethod::Source,
    };

    // When a source build is explicitly requested through the update API.
    f.hw.update_with("tool", &request, &NullSink, &CancellationToken::new())
        .await
        .unwrap();

    // Then the real build replaces the same executable and leaves configuration intact.
    crate::recovery::reconcile(f.hw.paths(), &app).unwrap();
    f.assert_payload(&f.new, "new");
    assert!(!alternate.exists());
    assert_eq!(f.hw.get("tool").unwrap().app, app);
    f.capture("forced-source");
}

#[tokio::test]
async fn failed_forced_asset_verification_preserves_the_existing_installation() {
    // Given an installed source build and a release recipe whose verifier rejects it.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    f.release(true).await;
    let app = with_github(f.hw.get("tool").unwrap().app, |mut source| {
        source.options.release_asset = Some(AssetInstall {
            verify_command: Some("exit 7".into()),
            ..Default::default()
        });
        source
    });
    f.hw.replace(app.clone()).unwrap();

    // When the forced download fails verification after replacement.
    let result =
        f.hw.update_with(
            "tool",
            &UpdateRequest {
                target: Target::Ref(f.new.clone()),
                method: RevisionMethod::Asset,
            },
            &NullSink,
            &CancellationToken::new(),
        )
        .await;

    // Then the old executable and baseline are restored and the failure is recorded.
    assert!(result.is_err());
    crate::recovery::reconcile(f.hw.paths(), &app).unwrap();
    f.assert_payload(&f.old, "old");
    assert!(!f.hw.history("tool").unwrap()[0].ok);
    f.capture("forced-asset-verification-failed");
}
