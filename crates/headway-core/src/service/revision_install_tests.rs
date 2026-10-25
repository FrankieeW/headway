use crate::paths::Paths;
use crate::shell::ShellEnv;
use crate::testing::{GitFixture, cli_app, with_github};
use crate::token::MemoryStore;
use crate::{
    CancellationToken, Error, Headway, Install, InstalledRef, NullSink, RevisionMethod,
    RevisionRequest, SourceInstall, UpdateEvent,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{fs, path::PathBuf, process::Command, sync::Arc};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};
fn script(value: &str) -> String {
    format!("#!/bin/sh\nprintf '%s\\n' {value}\n")
}
fn request(sha: &str, method: RevisionMethod) -> RevisionRequest {
    RevisionRequest {
        reference: sha.into(),
        method,
        source_build: None,
        release_asset: None,
        release_tag: None,
    }
}
fn record(name: &str, value: &impl serde::Serialize) {
    if let Some(dir) = std::env::var_os("HEADWAY_REVISION_EVIDENCE") {
        fs::write(
            PathBuf::from(dir).join(format!("{name}.json")),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    repo: GitFixture,
    server: MockServer,
    hw: Headway,
    old: String,
    new: String,
    dest: PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitFixture::new();
        let old = repo.commit("payload", &script("old"), "old payload");
        let new = repo.commit("payload", &script("new"), "new payload");
        let server = MockServer::start().await;
        for sha in [&old, &new] {
            let body = json!({"sha":sha,"commit":{"message":"fixture revision"}});
            remote(&server, &format!("commits/{sha}"), body).await;
        }
        remote(&server, "tags", json!([])).await;
        remote(&server, "releases", json!([])).await;
        let hw = Headway::builder()
            .paths(Paths::under(dir.path()))
            .api(server.uri())
            .secrets(Arc::new(MemoryStore::with_token("fixture-token")))
            .env(ShellEnv::default())
            .build()
            .unwrap();
        let dest = dir.path().join("bin/tool");
        hw.add(with_github(cli_app("tool"), |mut source| {
            source.git_url = Some(repo.url());
            source.install = Install::Source(SourceInstall {
                build: "cp payload tool && chmod +x tool".into(),
                artifact: Some("tool".into()),
                dest: Some(dest.display().to_string()),
                ..Default::default()
            });
            source
        }))
        .unwrap();
        Self {
            dir,
            repo,
            server,
            hw,
            old,
            new,
            dest,
        }
    }
    async fn install(&self, sha: &str) -> crate::UpdateOutcome {
        self.hw
            .install_revision(
                "tool",
                &request(sha, RevisionMethod::Source),
                &NullSink,
                &CancellationToken::new(),
            )
            .await
            .unwrap()
    }
    fn installed(&self) -> InstalledRef {
        let state = self.hw.store().state().unwrap();
        state.app("tool").installed.unwrap()
    }
    fn assert_payload(&self, sha: &str, value: &str) {
        let installed = self.installed();
        assert_eq!(installed.id, sha);
        assert_eq!(installed.sha.as_deref(), Some(sha));
        assert_eq!(fs::read(&self.dest).unwrap(), script(value).as_bytes());
        let output = Command::new(&self.dest).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, format!("{value}\n").as_bytes());
    }
    fn capture(&self, name: &str) {
        let history = self.hw.history("tool").unwrap();
        record(
            name,
            &json!({"installed":self.installed(),"bytes":fs::read_to_string(&self.dest).unwrap(),"history":history}),
        );
        if let Some(root) = std::env::var_os("HEADWAY_REVISION_EVIDENCE")
            && let Some(log) = history.first().and_then(|entry| entry.log_path.as_ref())
        {
            fs::copy(log, PathBuf::from(root).join(format!("{name}.log"))).unwrap();
        }
    }
    async fn release(&self, valid_digest: bool) -> String {
        self.repo.tag("v1.2.3");
        let payload = self.dir.path().join("release");
        fs::create_dir(&payload).unwrap();
        fs::write(payload.join("tool"), script("download")).unwrap();
        crate::archive::set_executable(&payload.join("tool")).unwrap();
        let archive = self.dir.path().join("tool-macos-universal.tar.gz");
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&payload)
            .arg("tool")
            .status()
            .unwrap();
        assert!(status.success());
        let bytes = fs::read(archive).unwrap();
        let digest = format!("sha256:{}", crate::github::to_hex(&Sha256::digest(&bytes)));
        let published = if valid_digest {
            digest.clone()
        } else {
            format!("sha256:{}", "0".repeat(64))
        };
        let release = json!({"id":1,"tag_name":"v1.2.3","target_commitish":"main","assets":[{"id":77,"name":"tool-macos-universal.tar.gz","size":bytes.len(),"digest":published}]});
        remote(
            &self.server,
            "tags",
            json!([{"name":"v1.2.3","commit":{"sha":self.new}}]),
        )
        .await;
        remote(&self.server, "releases", json!([release.clone()])).await;
        remote(&self.server, "releases/tags/v1.2.3", release).await;
        remote(
            &self.server,
            "commits/refs/tags/v1.2.3",
            json!({"sha":self.new,"commit":{"message":"release"}}),
        )
        .await;
        Mock::given(path("/repos/owner/tool/releases/assets/77"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&self.server)
            .await;
        digest
    }
}
async fn remote(server: &MockServer, endpoint: &str, body: Value) {
    let priority = if body.as_array().is_some_and(Vec::is_empty) {
        5
    } else {
        2
    };
    let expected = format!("/repos/owner/tool/{endpoint}");
    Mock::given(move |request: &wiremock::Request| {
        request.url.path().replace("%2F", "/").replace("%2f", "/") == expected
    })
    .respond_with(ResponseTemplate::new(200).set_body_json(body))
    .with_priority(priority)
    .mount(server)
    .await;
}

#[tokio::test]
async fn source_upgrade_installs_exact_sha_and_new_executable_bytes() {
    // Given an actual old build installed from a temporary Git repository.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    // When installing a newer exact commit.
    let done = f.install(&f.new).await;
    // Then the real executable and recorded ref both advance.
    assert_eq!(done.from.unwrap().sha.as_deref(), Some(f.old.as_str()));
    f.assert_payload(&f.new, "new");
    f.capture("upgrade");
}

#[tokio::test]
async fn source_downgrade_checks_out_old_sha_despite_new_remote_head() {
    // Given the newer commit installed while the upstream remains at that tip.
    let f = Fixture::new().await;
    f.install(&f.new).await;
    // When explicitly installing the older commit.
    let done = f.install(&f.old).await;
    // Then both executable content and installed ref downgrade exactly.
    assert_eq!(done.from.unwrap().sha.as_deref(), Some(f.new.as_str()));
    f.assert_payload(&f.old, "old");
    f.capture("downgrade");
}

#[tokio::test]
async fn one_off_recipe_plan_and_install_preserve_primary_config_bytes() {
    // Given a persistent configured recipe and a distinct temporary recipe.
    let f = Fixture::new().await;
    let before = fs::read(f.hw.paths().config_file()).unwrap();
    let mut chosen = request(&f.new, RevisionMethod::Source);
    chosen.source_build = Some(SourceInstall {
        build: "mkdir -p custom && sed 's/new/override/g' payload > custom/tool && chmod +x custom/tool".into(),
        artifact: Some("custom/tool".into()),
        ..Default::default()
    });
    let plan = f.hw.plan_revision("tool", &chosen).await.unwrap();
    assert_eq!(plan.to.sha.as_deref(), Some(f.new.as_str()));
    assert_eq!(plan.method, "source");
    assert!(!f.dest.exists(), "planning must not install");
    // When the temporary recipe is executed.
    f.hw.install_revision("tool", &chosen, &NullSink, &CancellationToken::new())
        .await
        .unwrap();
    // Then distinct override bytes prove the one-off recipe actually ran.
    f.assert_payload(&f.new, "override");
    assert_eq!(fs::read(f.hw.paths().config_file()).unwrap(), before);
    record("one-off-plan", &plan);
    f.capture("one-off");
}

#[tokio::test]
async fn verifier_failure_retains_old_executable_and_exact_installed_reference() {
    // Given a working old installation and a one-off verifier rejecting replacement.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    let previous = f.installed();
    let mut chosen = request(&f.new, RevisionMethod::Source);
    chosen.source_build = Some(SourceInstall {
        build: "cp payload tool && chmod +x tool".into(),
        artifact: Some("tool".into()),
        verify_command: Some("exit 19".into()),
        ..Default::default()
    });
    // When build/install succeeds but the verifier fails.
    let error =
        f.hw.install_revision("tool", &chosen, &NullSink, &CancellationToken::new())
            .await
            .unwrap_err();
    // Then rollback restores bytes and complete installed metadata.
    assert!(matches!(
        error,
        Error::CommandFailed {
            status: Some(19),
            ..
        }
    ));
    assert_eq!(f.installed(), previous);
    f.assert_payload(&f.old, "old");
    assert!(!f.hw.history("tool").unwrap().first().unwrap().ok);
    f.capture("verification-failure");
}

#[tokio::test]
async fn cancellation_during_verification_restores_old_bytes_and_ref() {
    // Given a verifier that signals its real subprocess has started.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    let previous = f.installed();
    let cancel = CancellationToken::new();
    let sink = |event: UpdateEvent| {
        if matches!(event, UpdateEvent::Log { line, .. } if line == "verifier-ready") {
            cancel.cancel();
        }
    };
    let mut chosen = request(&f.new, RevisionMethod::Source);
    chosen.source_build = Some(SourceInstall {
        build: "cp payload tool && chmod +x tool".into(),
        artifact: Some("tool".into()),
        verify_command: Some("echo verifier-ready; sleep 30".into()),
        ..Default::default()
    });
    // When cancellation arrives from verifier output, rather than a timing sleep.
    let error =
        f.hw.install_revision("tool", &chosen, &sink, &cancel)
            .await
            .unwrap_err();
    // Then cancellation is returned after exact rollback.
    assert!(cancel.is_cancelled());
    assert!(matches!(error, Error::Cancelled));
    assert_eq!(f.installed(), previous);
    f.assert_payload(&f.old, "old");
    f.capture("cancelled");
}

#[tokio::test]
async fn app_lock_rejects_revision_install_before_network_or_state_changes() {
    // Given an installed program whose app lock is already held.
    let f = Fixture::new().await;
    f.install(&f.old).await;
    let previous = f.installed();
    let count = f.server.received_requests().await.unwrap().len();
    let _held = crate::lock::AppLock::acquire(&f.hw.paths().app_lock("tool"), "tool").unwrap();
    // When another install targets that app.
    let error =
        f.hw.install_revision(
            "tool",
            &request(&f.new, RevisionMethod::Source),
            &NullSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    // Then it is busy without starting network/build work or changing installation.
    assert!(matches!(error, Error::Busy(_)));
    assert_eq!(f.server.received_requests().await.unwrap().len(), count);
    assert_eq!(f.installed(), previous);
    f.assert_payload(&f.old, "old");
    f.capture("busy");
}

#[path = "revision_install_release_tag_tests.rs"]
mod release_tags;

#[path = "update_method_install_tests.rs"]
mod update_methods;
