use super::*;

#[tokio::test]
async fn same_commit_source_and_release_download_choose_distinct_payloads_but_record_sha() {
    // Given an actual Git tag and digest-published tarball with a distinct binary.
    let f = Fixture::new().await;
    let digest = f.release(true).await;
    let config = fs::read(f.hw.paths().config_file()).unwrap();
    let options = f.hw.revision_options("tool", &f.new).await.unwrap();
    assert_eq!(options.commit, f.new);
    assert_eq!(options.release_tag.as_deref(), Some("v1.2.3"));
    assert!(options.can_download && options.source_build.is_some());
    f.install(&f.new).await;
    f.assert_payload(&f.new, "new");
    let chosen = request(&f.new, RevisionMethod::Asset);
    let plan = f.hw.plan_revision("tool", &chosen).await.unwrap();
    assert_eq!(plan.method, "asset");
    assert_eq!(
        plan.asset.as_ref().unwrap().digest.as_deref(),
        Some(digest.as_str())
    );
    // When downloading that same commit's release instead of rebuilding.
    let done =
        f.hw.install_revision("tool", &chosen, &NullSink, &CancellationToken::new())
            .await
            .unwrap();
    // Then requests use the tag, while installed state/history keep the commit SHA.
    f.assert_payload(&f.new, "download");
    assert_eq!(done.to.id, f.new);
    assert_eq!(f.hw.history("tool").unwrap().first().unwrap().to, f.new);
    assert_eq!(fs::read(f.hw.paths().config_file()).unwrap(), config);
    let requests: Vec<_> = f
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert!(
        requests
            .iter()
            .any(|p| p == "/repos/owner/tool/releases/tags/v1.2.3")
    );
    assert!(
        requests
            .iter()
            .any(|p| p == "/repos/owner/tool/releases/assets/77")
    );
    assert!(
        !requests
            .iter()
            .any(|p| p == &format!("/repos/owner/tool/releases/tags/{}", f.new))
    );
    record("release-options", &options);
    record("release-plan", &plan);
    record("release-requests", &requests);
    f.capture("release-download");
}

#[tokio::test]
async fn wrong_download_digest_cannot_replace_previous_installation() {
    // Given old installed bytes and a real archive with an intentionally wrong digest.
    let f = Fixture::new().await;
    f.release(false).await;
    f.install(&f.old).await;
    let previous = f.installed();
    // When the matching release archive is downloaded for the newer commit.
    let error =
        f.hw.install_revision(
            "tool",
            &request(&f.new, RevisionMethod::Asset),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    // Then digest verification, not just archive parsing, protects the installation.
    assert!(matches!(error, Error::Checksum { .. }));
    assert_eq!(f.installed(), previous);
    f.assert_payload(&f.old, "old");
    f.capture("digest-failure");
}

async fn publish_outside_tag_window(f: &Fixture, tag: &str, tagged_sha: &str, draft: bool) {
    let digest = f.release(true).await;
    f.repo.git(&["tag", tag, tagged_sha]);
    f.repo.git(&["branch", tag, &f.new]);
    // Force both bounded discovery lists empty; only direct release-by-tag can find it.
    for endpoint in ["tags", "releases"] {
        Mock::given(path(format!("/repos/owner/tool/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .with_priority(1)
            .mount(&f.server)
            .await;
    }
    let size = fs::metadata(f.dir.path().join("tool-macos-universal.tar.gz"))
        .unwrap()
        .len();
    let release = json!({"id":2,"tag_name":tag,"draft":draft,"target_commitish":"main","assets":[{"id":77,"name":"tool-macos-universal.tar.gz","size":size,"digest":digest}]});
    remote(&f.server, &format!("releases/tags/{tag}"), release).await;
    remote(
        &f.server,
        &format!("commits/{tag}"),
        json!({"sha":f.new,"commit":{"message":"same-name branch tip"}}),
    )
    .await;
    remote(
        &f.server,
        &format!("commits/refs/tags/{tag}"),
        json!({"sha":tagged_sha,"commit":{"message":"actual tag commit"}}),
    )
    .await;
}

#[tokio::test]
async fn old_tag_options_survive_full_sha_plan_and_asset_install() {
    // Given an actual tagged release absent from both bounded discovery windows.
    let f = Fixture::new().await;
    publish_outside_tag_window(&f, "v-old", &f.new, false).await;
    f.install(&f.old).await;
    let previous = f.installed();
    let config = fs::read(f.hw.paths().config_file()).unwrap();
    let options = f.hw.revision_options("tool", "v-old").await.unwrap();
    assert_eq!(options.commit, f.new);
    assert_eq!(options.release_tag.as_deref(), Some("v-old"));
    assert!(options.can_download);
    let mut chosen = request(&options.commit, RevisionMethod::Asset);
    chosen.release_tag = options.release_tag.clone();
    let plan = f.hw.plan_revision("tool", &chosen).await.unwrap();
    assert_eq!(plan.method, "asset");
    assert_eq!(plan.to.sha.as_deref(), Some(f.new.as_str()));
    assert_eq!(
        f.installed(),
        previous,
        "planning must not change the installed baseline"
    );
    // When the UI pins the SHA but carries forward the verified release tag.
    let done =
        f.hw.install_revision("tool", &chosen, &NullSink, &CancellationToken::new())
            .await
            .unwrap();
    // Then the real archive installs at the exact commit without editing primary config.
    f.assert_payload(&f.new, "download");
    assert_eq!(done.to.id, f.new);
    assert_eq!(fs::read(f.hw.paths().config_file()).unwrap(), config);
    let requests: Vec<_> = f
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request.url.path().replace("%2F", "/").replace("%2f", "/"))
        .collect();
    assert!(
        requests
            .iter()
            .any(|path| path == "/repos/owner/tool/releases/tags/v-old")
    );
    assert!(
        requests
            .iter()
            .any(|path| path == "/repos/owner/tool/releases/assets/77")
    );
    assert!(
        !requests
            .iter()
            .any(|path| path == &format!("/repos/owner/tool/releases/tags/{}", f.new))
    );
    record("old-tag-options", &options);
    record("old-tag-plan", &plan);
    record("old-tag-request", &chosen);
    record("old-tag-requests", &requests);
    f.capture("old-tag-download");
}

#[tokio::test]
async fn supplied_mismatched_or_draft_tag_is_rejected_before_payload_or_state_changes() {
    // Given a supplied tag pointing to another commit, or an unpublished draft.
    for draft in [false, true] {
        let f = Fixture::new().await;
        let tag = if draft { "v-draft" } else { "v-wrong" };
        let tagged_sha = if draft { &f.new } else { &f.old };
        publish_outside_tag_window(&f, tag, tagged_sha, draft).await;
        f.install(&f.old).await;
        let previous = f.installed();
        let config = fs::read(f.hw.paths().config_file()).unwrap();
        let history_count = f.hw.history("tool").unwrap().len();
        for method in [RevisionMethod::Asset, RevisionMethod::Source] {
            let mut chosen = request(&f.new, method);
            chosen.release_tag = Some(tag.into());
            // When planning or installing a pinned SHA with that unverified association.
            let plan = f.hw.plan_revision("tool", &chosen).await;
            let result =
                f.hw.install_revision("tool", &chosen, &NullSink, &CancellationToken::new())
                    .await;
            // Then prepare rejects both paths without downloading or changing installed/config state.
            assert!(matches!(plan, Err(Error::InvalidInput(_))));
            assert!(matches!(result, Err(Error::InvalidInput(_))));
            assert_eq!(f.installed(), previous);
            f.assert_payload(&f.old, "old");
            assert_eq!(f.hw.history("tool").unwrap().len(), history_count);
            assert_eq!(fs::read(f.hw.paths().config_file()).unwrap(), config);
            let requests: Vec<_> = f
                .server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .map(|request| request.url.path().replace("%2F", "/").replace("%2f", "/"))
                .collect();
            assert!(
                requests
                    .iter()
                    .any(|path| path == &format!("/repos/owner/tool/releases/tags/{tag}"))
            );
            assert!(
                !requests
                    .iter()
                    .any(|path| path == "/repos/owner/tool/releases/assets/77")
            );
            let base_name = if draft {
                "rejected-draft-tag"
            } else {
                "rejected-mismatched-tag"
            };
            if !draft {
                assert!(
                    requests
                        .iter()
                        .any(|path| path == &format!("/repos/owner/tool/commits/refs/tags/{tag}"))
                );
            }
            let name = format!("{base_name}-{method:?}");
            record(&format!("{name}-requests"), &requests);
            f.capture(&name);
        }
    }
}

#[tokio::test]
async fn tag_tracking_source_commit_keeps_known_tag_baseline_and_checks_up_to_date() {
    // Given a real version tag on a commit, without a corresponding GitHub release.
    let f = Fixture::new().await;
    f.repo.tag("v2.0.0");
    remote(
        &f.server,
        "tags",
        json!([{"name":"v2.0.0","commit":{"sha":f.new}}]),
    )
    .await;
    let configured = f.hw.get("tool").unwrap().app;
    f.hw.replace(with_github(configured, |mut source| {
        source.track = crate::Track::Tag {
            prerelease: false,
            tag_filter: None,
        };
        source
    }))
    .unwrap();
    let config = fs::read(f.hw.paths().config_file()).unwrap();
    let options = f.hw.revision_options("tool", &f.new).await.unwrap();
    assert!(options.release_tag.is_none() && !options.can_download);
    // When that exact commit is built and its tag-tracked source is checked again.
    let done =
        f.hw.install_revision(
            "tool",
            &request(&f.new, RevisionMethod::Source),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let checked = f.hw.check("tool").await.unwrap();
    record("tag-source-check", &checked);
    record("tag-source-install", &done);
    f.capture("tag-source-baseline");
    // Then the actual tag remains the baseline, with exact bytes/SHA and no phantom update.
    assert_eq!(fs::read(&f.dest).unwrap(), script("new").as_bytes());
    let output = Command::new(&f.dest).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"new\n");
    assert_eq!(done.to.id, "v2.0.0");
    assert_eq!(f.installed().id, "v2.0.0");
    assert_eq!(f.installed().sha.as_deref(), Some(f.new.as_str()));
    assert_eq!(checked.status, crate::AppStatus::UpToDate);
    assert_eq!(
        checked.last_check.unwrap().status,
        crate::UpdateStatus::UpToDate
    );
    assert_eq!(checked.pending_count, 0);
    assert_eq!(fs::read(f.hw.paths().config_file()).unwrap(), config);
}

#[tokio::test]
async fn release_tracking_source_build_preserves_explicit_old_release_tag_baseline() {
    // Given an undiscoverable published release on a release-tracked source app.
    let f = Fixture::new().await;
    publish_outside_tag_window(&f, "v-old", &f.new, false).await;
    let configured = f.hw.get("tool").unwrap().app;
    f.hw.replace(with_github(configured, |mut source| {
        source.track = crate::Track::Release {
            prerelease: false,
            tag_filter: None,
        };
        source
    }))
    .unwrap();
    let config = fs::read(f.hw.paths().config_file()).unwrap();
    let options = f.hw.revision_options("tool", "v-old").await.unwrap();
    let mut chosen = request(&options.commit, RevisionMethod::Source);
    chosen.release_tag = options.release_tag;
    // When choosing a real source build while carrying the validated old release tag.
    let done =
        f.hw.install_revision("tool", &chosen, &NullSink, &CancellationToken::new())
            .await
            .unwrap();
    // Then the baseline remains the actual release tag and the build uses its exact SHA.
    assert_eq!(done.to.id, "v-old");
    assert_eq!(f.installed().id, "v-old");
    assert_eq!(f.installed().sha.as_deref(), Some(f.new.as_str()));
    assert_eq!(fs::read(&f.dest).unwrap(), script("new").as_bytes());
    let output = Command::new(&f.dest).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"new\n");
    assert_eq!(fs::read(f.hw.paths().config_file()).unwrap(), config);
    assert!(
        !f.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path() == "/repos/owner/tool/releases/assets/77")
    );
    record("old-tag-source-request", &chosen);
    record("old-tag-source-install", &done);
    f.capture("old-tag-source-baseline");
}
