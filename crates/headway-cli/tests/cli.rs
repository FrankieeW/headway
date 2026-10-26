//! End-to-end runs of the `headway` binary against a mocked GitHub API and a
//! local upstream git repository: no network, real git, real builds.
#![allow(clippy::unwrap_used, clippy::panic)] // test helpers fail loudly by design

use std::path::Path;
use std::process::Command as Process;

use assert_cmd::Command;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BUILD: &str = "mkdir -p out && printf '#!/bin/sh\\necho %s\\n' \"$(cat VERSION)\" > out/tool && chmod +x out/tool";

/// An upstream repository whose build produces `out/tool`, printing `VERSION`.
struct Upstream {
    dir: tempfile::TempDir,
}

impl Upstream {
    fn new() -> Self {
        let upstream = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        upstream.git(&["init", "-q", "-b", "main"]);
        upstream
    }

    fn url(&self) -> String {
        format!("file://{}", self.dir.path().display())
    }

    fn commit(&self, version: &str) -> String {
        std::fs::write(self.dir.path().join("VERSION"), version).unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", &format!("release {version}")]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Process::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@headway.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

fn commit_json(sha: &str, message: &str) -> Value {
    json!({
        "sha": sha,
        "html_url": format!("https://github.com/octo/tool/commit/{sha}"),
        "commit": {
            "message": message,
            "author": {"name": "Test", "date": "2026-10-01T00:00:00Z"},
            "committer": {"name": "Test", "date": "2026-10-01T00:00:00Z"}
        },
        "author": null
    })
}

/// Mocks the GitHub endpoints for a linear history `commits` (oldest first).
async fn mock_github(server: &MockServer, commits: &[(String, String)]) {
    let respond = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    let (head, head_message) = commits.last().unwrap();
    Mock::given(method("GET"))
        .and(path("/repos/octo/tool"))
        .respond_with(respond(json!({
            "full_name": "octo/tool", "default_branch": "main", "html_url": "https://github.com/octo/tool"
        })))
        .mount(server)
        .await;
    Mock::given(path("/repos/octo/tool/commits/main"))
        .respond_with(respond(commit_json(head, head_message)))
        .mount(server)
        .await;
    let newest_first: Vec<Value> = commits
        .iter()
        .rev()
        .map(|(sha, msg)| commit_json(sha, msg))
        .collect();
    Mock::given(path("/repos/octo/tool/commits"))
        .respond_with(respond(Value::Array(newest_first)))
        .mount(server)
        .await;
    for (index, (sha, message)) in commits.iter().enumerate() {
        Mock::given(path(format!("/repos/octo/tool/commits/{sha}")))
            .respond_with(respond(commit_json(sha, message)))
            .mount(server)
            .await;
        let ahead: Vec<Value> = commits[index + 1..]
            .iter()
            .map(|(sha, msg)| commit_json(sha, msg))
            .collect();
        Mock::given(path(format!("/repos/octo/tool/compare/{sha}...{head}")))
            .respond_with(respond(json!({
                "status": if ahead.is_empty() { "identical" } else { "ahead" },
                "ahead_by": ahead.len(), "behind_by": 0, "commits": ahead
            })))
            .mount(server)
            .await;
    }
}

fn headway(home: &Path, api: &str) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("headway"));
    command
        .env("HEADWAY_HOME", home)
        .env("HEADWAY_API_URL", api)
        // A dummy token keeps the keychain and `gh` out of the test.
        .env("HEADWAY_GITHUB_TOKEN", "ghp_dummytokendummytokendummy0000")
        .env("NO_COLOR", "1")
        .env_remove("HEADWAY_OUTPUT");
    command
}

fn json_of(command: &mut Command) -> Value {
    let output = command.output().unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "not JSON ({err}): {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_installed(bin: &Path) -> String {
    let output = Process::new(bin.join("tool")).output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn follows_a_repository_one_commit_at_a_time() {
    let upstream = Upstream::new();
    std::fs::write(upstream.dir.path().join(".gitignore"), "").unwrap();
    let commits: Vec<(String, String)> = ["v1", "v2", "v3"]
        .iter()
        .map(|version| (upstream.commit(version), format!("release {version}")))
        .collect();
    let server = MockServer::start().await;
    mock_github(&server, &commits).await;

    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    let hw = || headway(home.path(), &server.uri());

    hw().args(["settings", "set", "bin_dir", &bin.display().to_string()])
        .assert()
        .success();
    hw().args([
        "add",
        "octo/tool",
        "--no-inspect",
        "--git-url",
        &upstream.url(),
        "--build",
        BUILD,
        "--artifact",
        "out/tool",
    ])
    .assert()
    .success();

    // Baseline: v1 is what we have; nothing gets built.
    hw().args(["mark", "tool", &commits[0].0])
        .assert()
        .success();
    assert!(!bin.join("tool").exists());

    let checked = json_of(hw().args(["--json", "check"]));
    assert_eq!(checked["ok"], true);
    assert_eq!(checked["data"]["results"][0]["status"], "update_available");
    assert_eq!(checked["data"]["results"][0]["pending_count"], 2);
    assert!(
        checked["next"]
            .as_array()
            .unwrap()
            .iter()
            .any(|next| next == "headway --json update tool --next")
    );

    // One step: exactly the next commit.
    let stepped = json_of(hw().args(["--json", "update", "tool", "--next"]));
    assert_eq!(stepped["ok"], true, "{stepped}");
    assert_eq!(stepped["data"]["results"][0]["to"]["id"], commits[1].0);
    assert_eq!(run_installed(&bin), "v2");

    let shown = json_of(hw().args(["--json", "show", "tool"]));
    assert_eq!(shown["data"]["installed"]["id"], commits[1].0);
    assert_eq!(shown["data"]["pending_count"], 1);

    // Then all the way to the latest.
    hw().args(["update", "tool"]).assert().success();
    assert_eq!(run_installed(&bin), "v3");

    let again = json_of(hw().args(["--json", "update", "tool"]));
    assert_eq!(again["data"]["results"][0]["changed"], false);

    let history = json_of(hw().args(["--json", "history", "tool"]));
    let actions: Vec<(&str, &str)> = history["data"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["action"].as_str().unwrap(),
                entry["to"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        actions,
        [
            ("update", commits[2].0.as_str()),
            ("update", commits[1].0.as_str()),
            ("mark", commits[0].0.as_str())
        ]
    );

    let log = json_of(hw().args(["--json", "log", "tool"]));
    assert!(
        log["data"]["content"]
            .as_str()
            .unwrap()
            .contains("Running `mkdir -p out")
    );

    let xml = hw().args(["--ai", "list"]).output().unwrap();
    let xml = String::from_utf8_lossy(&xml.stdout);
    assert!(
        xml.starts_with("<headway command=\"list\" ok=\"true\""),
        "{xml}"
    );
    assert!(
        xml.contains("id=\"tool\"") && xml.contains("status=\"up_to_date\""),
        "{xml}"
    );

    // Roll back to an older commit explicitly: the newer ones show up as pending again.
    hw().args(["update", "tool", "--to", &commits[0].0])
        .assert()
        .success();
    assert_eq!(run_installed(&bin), "v1");
    let rolled_back = json_of(hw().args(["--json", "show", "tool"]));
    assert_eq!(rolled_back["data"]["status"], "update_available");
    assert_eq!(rolled_back["data"]["pending_count"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn failures_are_structured_for_agents() {
    let upstream = Upstream::new();
    let commits = vec![(upstream.commit("v1"), "release v1".to_owned())];
    let server = MockServer::start().await;
    mock_github(&server, &commits).await;
    let home = tempfile::tempdir().unwrap();
    let hw = || headway(home.path(), &server.uri());

    let missing = hw().args(["--json", "show", "nope"]).assert().code(3);
    let missing: Value = serde_json::from_slice(&missing.get_output().stdout).unwrap();
    assert_eq!(missing["ok"], false);
    assert_eq!(missing["error"]["code"], "app_not_found");
    assert!(
        missing["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("headway list")
    );

    let usage = hw().args(["--ai", "update", "--bogus"]).assert().code(2);
    let usage = String::from_utf8_lossy(&usage.get_output().stdout).into_owned();
    assert!(usage.contains("code=\"usage_error\""), "{usage}");

    hw().args([
        "add",
        "octo/tool",
        "--no-inspect",
        "--git-url",
        &upstream.url(),
        "--build",
        "echo boom >&2; exit 3",
    ])
    .assert()
    .success();
    let failed = hw().args(["--json", "update", "tool"]).assert().code(5);
    let failed: Value = serde_json::from_slice(&failed.get_output().stdout).unwrap();
    assert_eq!(failed["error"]["code"], "command_failed");
    assert!(
        failed["error"]["details"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line == "boom")
    );

    let shown = json_of(hw().args(["--json", "show", "tool"]));
    assert_eq!(shown["data"]["status"], "error");
    assert_eq!(shown["data"]["history"][0]["ok"], false);

    // A download pattern is another way to install this repository. The saved default stays the build.
    let edited = json_of(hw().args(["--json", "edit", "tool", "--pattern", "*.tar.gz"]));
    assert_eq!(edited["ok"], true);
    assert_eq!(edited["data"]["app"]["install"]["method"], "source");
    assert_eq!(
        edited["data"]["app"]["release_asset"]["pattern"],
        "*.tar.gz"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sigterm_stops_the_build_and_releases_the_lock() {
    let upstream = Upstream::new();
    let commits = vec![(upstream.commit("v1"), "release v1".to_owned())];
    let server = MockServer::start().await;
    mock_github(&server, &commits).await;
    let home = tempfile::tempdir().unwrap();
    let pid_file = home.path().join("build.pid");
    // `exec` makes the recorded PID the long-running build itself.
    let build = format!("echo $$ > '{}' && exec sleep 60", pid_file.display());
    headway(home.path(), &server.uri())
        .args([
            "add",
            "octo/tool",
            "--no-inspect",
            "--git-url",
            &upstream.url(),
            "--build",
            &build,
        ])
        .assert()
        .success();

    let cli = assert_cmd::cargo::cargo_bin("headway");
    let child = Process::new(cli)
        .args(["--json", "update", "tool"])
        .env("HEADWAY_HOME", home.path())
        .env("HEADWAY_API_URL", server.uri())
        .env("HEADWAY_GITHUB_TOKEN", "ghp_dummytokendummytokendummy0000")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !pid_file.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the build never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let build_pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .to_owned();
    Process::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(7), "cancelled exit code");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["error"]["code"], "cancelled");
    let build_alive = Process::new("kill")
        .args(["-0", &build_pid])
        .status()
        .unwrap()
        .success();
    assert!(!build_alive, "the build process must not outlive the CLI");
    let shown = json_of(headway(home.path(), &server.uri()).args(["--json", "show", "tool"]));
    assert_eq!(shown["data"]["busy"], false);
}

#[test]
fn describes_itself() {
    let home = tempfile::tempdir().unwrap();
    let tree = json_of(headway(home.path(), "http://127.0.0.1:9").args(["commands", "--json"]));
    let names: Vec<&str> = tree["data"]["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sub| sub["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"update") && names.contains(&"mark") && names.contains(&"inspect"));
}
