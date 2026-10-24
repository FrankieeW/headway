use crate::{
    AutoUpdateMode, CancellationToken, Error, Headway, Install, NullSink, RevisionMethod,
    RevisionRequest, SourceInstall, Track, UpdateEvent, VariantRequest,
};
use crate::{
    paths::Paths,
    shell::ShellEnv,
    testing::{GitFixture, cli_app, with_github},
    token::MemoryStore,
};
use serde_json::json;
use std::{fs, path::PathBuf, sync::Arc};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

fn request(id: &str, sha: &str) -> VariantRequest {
    VariantRequest {
        id: id.into(),
        name: None,
        revision: revision(sha),
    }
}
fn revision(sha: &str) -> RevisionRequest {
    RevisionRequest {
        release_tag: None,
        reference: sha.into(),
        method: RevisionMethod::Source,
        source_build: None,
        release_asset: None,
    }
}
fn payload(value: &str) -> String {
    format!("#!/bin/sh\nprintf '%s\\n' {value}\n")
}

struct Fixture {
    _root: tempfile::TempDir,
    _repo: GitFixture,
    _server: MockServer,
    hw: Headway,
    new: String,
    next: String,
    parent_path: PathBuf,
    bin: PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = GitFixture::new();
        let old = repo.commit("payload", &payload("old"), "old");
        let new = repo.commit("payload", &payload("new"), "new");
        let next = repo.commit("payload", &payload("next"), "next");
        let server = MockServer::start().await;
        for (endpoint, body) in [("tags".into(), json!([])), ("releases".into(), json!([]))]
            .into_iter()
            .chain([&old, &new, &next].into_iter().map(|sha| {
                (
                    format!("commits/{sha}"),
                    json!({"sha":sha,"commit":{"message":"fixture"}}),
                )
            }))
        {
            Mock::given(path(format!("/repos/owner/tool/{endpoint}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
        }
        let hw = Headway::builder()
            .paths(Paths::under(root.path()))
            .api(server.uri())
            .secrets(Arc::new(MemoryStore::with_token("fixture-token")))
            .env(
                ShellEnv::current()
                    .with_var("GIT_CONFIG_GLOBAL", "/dev/null")
                    .with_var("GIT_CONFIG_NOSYSTEM", "1"),
            )
            .build()
            .unwrap();
        let bin = root.path().join("bin");
        let parent_path = bin.join("tool");
        let mut settings = hw.settings().unwrap();
        settings.bin_dir = bin.display().to_string();
        settings.apps_dir = root.path().join("apps").display().to_string();
        hw.save_settings(&settings).unwrap();
        let mut parent = with_github(cli_app("tool"), |mut github| {
            github.git_url = Some(repo.url());
            github.install = Install::Source(SourceInstall {
                build: "cp payload tool && chmod +x tool".into(),
                artifact: Some("tool".into()),
                dest: Some(parent_path.display().to_string()),
                ..Default::default()
            });
            github
        });
        parent.auto_update = true;
        hw.add(parent).unwrap();
        hw.install_revision(
            "tool",
            &revision(&old),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        Self {
            _root: root,
            _repo: repo,
            _server: server,
            hw,
            new,
            next,
            parent_path,
            bin,
        }
    }
    fn parent_snapshot(&self) -> serde_json::Value {
        json!({"app":self.hw.store().config().unwrap().app("tool").unwrap(),
            "state":self.hw.store().state().unwrap().app("tool"),"bytes":fs::read(&self.parent_path).unwrap()})
    }
    async fn install(&self, id: &str) -> crate::UpdateOutcome {
        self.hw
            .install_variant(
                "tool",
                &request(id, &self.new),
                &NullSink,
                &CancellationToken::new(),
            )
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn independent_source_install_preserves_parent_and_pins_the_exact_child() {
    // Given a real installed parent and another resolvable Git commit.
    let f = Fixture::new().await;
    let parent = f.parent_snapshot();
    // When the public API installs an independent version.
    let outcome = f.install("tool-extra").await;
    // Then only a distinct destination and pinned child are created.
    assert_eq!(f.parent_snapshot(), parent);
    assert_eq!(outcome.to.sha.as_deref(), Some(f.new.as_str()));
    let config = f.hw.store().config().unwrap();
    let child = config.app("tool-extra").unwrap();
    assert!(!child.auto_update);
    assert_eq!(child.update_policy.mode, AutoUpdateMode::Notify);
    let github = child.github().unwrap();
    let variant = github.options.variant.as_ref().unwrap();
    assert_eq!(variant.parent_id, "tool");
    assert_eq!(variant.reference, f.new);
    assert_eq!(
        github.track,
        Track::Commit {
            branch: Some(f.new.clone())
        }
    );
    assert_eq!(
        fs::read(f.bin.join("tool-extra")).unwrap(),
        payload("new").as_bytes()
    );
    assert_ne!(
        outcome.installed_path.as_deref(),
        Some(f.parent_path.to_str().unwrap())
    );
    assert!(
        f.hw.launch_variant("tool-extra").await.is_err(),
        "CLI variants execute directly, not through app launch"
    );
    println!(
        "variant_independent={}",
        json!({"parent":parent,"child":child,"outcome":outcome})
    );
}

#[tokio::test]
async fn occupied_id_destination_and_parent_id_are_refused_without_mutation() {
    // Given an installed parent, an existing ID, and a user-owned destination.
    let f = Fixture::new().await;
    f.hw.add(cli_app("taken")).unwrap();
    fs::write(f.bin.join("occupied"), "user-data").unwrap();
    let parent = f.parent_snapshot();
    let config = f.hw.store().config().unwrap();
    // When each conflicting independent install is requested.
    for id in ["tool", "taken", "occupied"] {
        assert!(
            f.hw.install_variant(
                "tool",
                &request(id, &f.new),
                &NullSink,
                &CancellationToken::new()
            )
            .await
            .is_err()
        );
        // Then no registration, parent metadata or user bytes are changed.
        assert_eq!(f.hw.store().config().unwrap(), config);
        assert_eq!(f.parent_snapshot(), parent);
        assert_eq!(fs::read(f.bin.join("occupied")).unwrap(), b"user-data");
    }
}

#[tokio::test]
async fn self_installing_recipe_is_refused_before_child_registration() {
    // Given a parent recipe without a managed build artifact.
    let f = Fixture::new().await;
    let app = with_github(
        f.hw.store().config().unwrap().app("tool").unwrap().clone(),
        |mut github| {
            let Install::Source(ref mut source) = github.install else {
                unreachable!()
            };
            source.artifact = None;
            github
        },
    );
    f.hw.replace(app).unwrap();
    let parent = f.parent_snapshot();
    // When that recipe is requested as an independent install.
    assert!(
        f.hw.install_variant(
            "tool",
            &request("tool-extra", &f.new),
            &NullSink,
            &CancellationToken::new()
        )
        .await
        .is_err()
    );
    // Then no child is registered or source build invoked.
    assert_eq!(f.parent_snapshot(), parent);
    assert_eq!(f.hw.list().unwrap().len(), 1);
    assert!(!f.hw.paths().source_dir("tool-extra").exists());
}

#[tokio::test]
async fn independent_reinstall_changes_only_the_child_pin_and_payload() {
    // Given an independently installed exact revision.
    let f = Fixture::new().await;
    f.install("tool-extra").await;
    let parent = f.parent_snapshot();
    // When the same child is reinstalled at a different resolved SHA.
    let outcome =
        f.hw.install_revision(
            "tool-extra",
            &revision(&f.next),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    // Then its pin and bytes advance without changing the primary installation.
    assert_eq!(f.parent_snapshot(), parent);
    assert_eq!(outcome.to.sha.as_deref(), Some(f.next.as_str()));
    let config = f.hw.store().config().unwrap();
    let github = config.app("tool-extra").unwrap().github().unwrap();
    assert_eq!(github.options.variant.as_ref().unwrap().reference, f.next);
    assert_eq!(
        github.track,
        Track::Commit {
            branch: Some(f.next.clone())
        }
    );
    assert_eq!(
        fs::read(f.bin.join("tool-extra")).unwrap(),
        payload("next").as_bytes()
    );
}

#[tokio::test]
async fn failed_and_cancelled_independent_builds_preserve_the_primary_install() {
    // Given a parent and recipes that fail or wait for cancellation.
    for cancelled in [false, true] {
        let f = Fixture::new().await;
        let parent = f.parent_snapshot();
        let mut selected = request("tool-extra", &f.new);
        selected.revision.source_build = Some(SourceInstall {
            build: if cancelled {
                "echo variant-ready; sleep 30"
            } else {
                "exit 19"
            }
            .into(),
            artifact: Some("tool".into()),
            ..Default::default()
        });
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let sink = move |event: UpdateEvent| {
            if matches!(event,UpdateEvent::Log {line,..} if line=="variant-ready") {
                trigger.cancel();
            }
        };
        // When the independent build fails after child registration.
        let error =
            f.hw.install_variant("tool", &selected, &sink, &cancel)
                .await
                .unwrap_err();
        // Then the child reports failure while the primary remains byte/ref/config-identical.
        assert_eq!(matches!(error, Error::Cancelled), cancelled);
        assert_eq!(f.parent_snapshot(), parent);
        assert!(!f.bin.join("tool-extra").exists());
        let state = f.hw.store().state().unwrap().app("tool-extra");
        assert!(state.installed.is_none());
        assert!(!state.history.last().unwrap().ok);
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn launch_preserves_shared_home_and_variant_marker_without_forwarding_token() {
    use std::io::{BufRead as _, Write as _};
    use std::os::unix::fs::PermissionsExt as _;
    // Given a disposable app bundle and an event pipe, without data/profile overrides.
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("event");
    let env = ShellEnv::from_vars([
        ("HOME".into(), "fixture-shared-home".into()),
        ("TEST_FIFO".into(), fifo.display().to_string()),
        (
            "HEADWAY_VARIANT_ID".into(),
            "inherited-parent-marker".into(),
        ),
        (
            "HEADWAY_GITHUB_TOKEN".into(),
            "fixture-not-a-real-token".into(),
        ),
    ]);
    let output = crate::process::capture(
        &crate::process::Cmd::new("/usr/bin/mkfifo", &env).arg(fifo.display().to_string()),
    )
    .await
    .unwrap();
    assert!(output.success());
    let bundle = root.path().join("Probe.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    fs::write(bundle.join("Contents/Info.plist"), "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>Probe</string></dict></plist>").unwrap();
    let executable = bundle.join("Contents/MacOS/Probe");
    fs::write(&executable, "#!/bin/sh\nprintf '%s|%s|%s\\n' \"$HOME\" \"$HEADWAY_VARIANT_ID\" \"${HEADWAY_GITHUB_TOKEN-unset}\" > \"$TEST_FIFO\"\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .env(env)
        .build()
        .unwrap();
    let mut app = with_github(cli_app("probe"), |mut github| {
        github.options.variant = Some(crate::VariantInfo {
            parent_id: "parent".into(),
            reference: "a".repeat(40),
            release_tag: None,
            branch: None,
        });
        github.install = Install::Source(SourceInstall {
            build: "true".into(),
            artifact: Some("Probe.app".into()),
            dest: Some(bundle.display().to_string()),
            ..Default::default()
        });
        github
    });
    app.kind = crate::AppKind::App;
    hw.add(app).unwrap();
    let pipe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(fifo)
        .unwrap();
    let mut unblock = pipe.try_clone().unwrap();
    let event = tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        std::io::BufReader::new(pipe).read_line(&mut line).unwrap();
        line
    });
    // When the public launch API starts only this fixture and emits its real environment.
    let launched = hw.launch_variant("probe").await;
    if launched.is_err() {
        writeln!(unblock, "launch-error").unwrap();
    }
    let observed = tokio::time::timeout(std::time::Duration::from_secs(5), event).await;
    if observed.is_err() {
        writeln!(unblock, "launch-timeout").unwrap();
    }
    let launched = launched.unwrap();
    // Then normal shared HOME remains intact, the child ID is marked, and token is absent.
    let event = observed.unwrap().unwrap();
    assert_eq!(event.trim(), "fixture-shared-home|probe|unset");
    assert_eq!(launched.path, bundle.display().to_string());
    assert!(launched.pid > 0);
    println!(
        "variant_launch={}",
        json!({"outcome":launched,"event":event,"home_preserved":true,"marker":"probe","token":"unset"})
    );
}

#[test]
fn running_variant_recognizes_an_alias_to_the_current_executable() {
    // Given an independent registration through a symlink alias, without installing payloads.
    let root = tempfile::tempdir().unwrap();
    let alias = root.path().join("executable-alias");
    std::os::unix::fs::symlink(std::env::current_exe().unwrap(), &alias).unwrap();
    let hw = Headway::builder()
        .paths(Paths::under(root.path()))
        .build()
        .unwrap();
    hw.add(with_github(cli_app("alias-variant"), |mut github| {
        let Install::Source(ref mut source) = github.install else {
            unreachable!()
        };
        source.dest = Some(alias.display().to_string());
        github.options.variant = Some(crate::VariantInfo {
            parent_id: "parent".into(),
            reference: "a".repeat(40),
            release_tag: None,
            branch: None,
        });
        github
    }))
    .unwrap();
    // When determining whether the current process is an independent version.
    let observed = hw.running_is_variant().unwrap();
    // Then canonical target/executable aliases match without payload mutation.
    assert!(observed);
}
