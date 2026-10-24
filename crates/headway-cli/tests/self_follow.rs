//! Real Headway commit advancement through the self-build preset, entirely offline.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use serde_json::json;
use std::{fs, process::Command};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[expect(
    dead_code,
    reason = "Shared fixture also serves the self_build integration target."
)]
mod self_build_support;
use self_build_support::{Repo, Stamp, success};

#[tokio::test(flavor = "multi_thread")]
async fn next_builds_and_installs_the_next_self_host_commit_without_losing_cache() {
    // Given two real commits containing the actual self-build helper.
    let repo = Repo::new();
    let first = repo.git(&["rev-parse", "HEAD"]);
    let second = repo.commit("second");
    let server = MockServer::start().await;
    let commit = |sha: &str| {
        json!({
            "sha": sha,
            "html_url": format!("https://github.com/octo/headway/commit/{sha}"),
            "commit": {"message": "fixture revision", "author": {"name": "Fixture", "date": "2026-10-01T00:00:00Z"},
                       "committer": {"name": "Fixture", "date": "2026-10-01T00:00:00Z"}},
            "author": null
        })
    };
    for (endpoint, body) in [
        (
            "/repos/octo/headway".to_owned(),
            json!({"full_name": "octo/headway", "default_branch": "main", "html_url": "https://github.com/octo/headway"}),
        ),
        (
            "/repos/octo/headway/commits/main".to_owned(),
            commit(&second),
        ),
        (
            "/repos/octo/headway/commits".to_owned(),
            json!([commit(&second), commit(&first)]),
        ),
        (
            format!("/repos/octo/headway/commits/{first}"),
            commit(&first),
        ),
        (
            format!("/repos/octo/headway/commits/{second}"),
            commit(&second),
        ),
        (
            format!("/repos/octo/headway/compare/{first}...{second}"),
            json!({"status": "ahead", "ahead_by": 1, "behind_by": 0, "commits": [commit(&second)]}),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    let destination = home.path().join("bin/headway");
    let run = |args: &[&str]| {
        let output = success(
            repo.headway(home.path(), &server.uri())
                .args(args)
                .output()
                .unwrap(),
        );
        println!(
            "headway {args:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    };
    let installed = || {
        let output = success(
            Command::new(&destination)
                .env_clear()
                .args(["--json", "version"])
                .output()
                .unwrap(),
        );
        let stamp: Stamp = serde_json::from_slice(&output.stdout).unwrap();
        println!(
            "installed executable: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        stamp
    };
    run(&[
        "add",
        "octo/headway",
        "--no-inspect",
        "--preset",
        "headway-cli",
        "--git-url",
        &format!("file://{}", repo.root.display()),
        "--branch",
        "main",
        "--dest",
        destination.to_str().unwrap(),
    ]);
    run(&["update", "headway", "--to", &first]);
    assert_eq!(installed().commit, first);
    let sentinel = home
        .path()
        .join("cache/sources/headway/target/cache-sentinel");
    fs::write(&sentinel, "retained compiler cache").unwrap();

    // When the engine advances using --next rather than an explicit revision.
    run(&["update", "headway", "--next"]);

    // Then the installed executable changes to the full next SHA and cache survives.
    let stamp = installed();
    assert_ne!(first, second);
    assert_eq!(stamp.commit, second);
    assert!(!stamp.dirty);
    assert_eq!(stamp.target, "aarch64-apple-darwin");
    assert_eq!(
        fs::read_to_string(sentinel).unwrap(),
        "retained compiler cache"
    );
    println!("cache sentinel retained after --next");
}
