use super::*;
use crate::testing::{FakeTools, GitFixture};

#[tokio::test]
async fn clone_retry_removes_partial_git_directory_before_replay() {
    let work = tempfile::tempdir().unwrap();
    let dir = work.path().join("checkout");
    let attempts = work.path().join("attempts");
    let tools = FakeTools::new().tool(
        "git",
        r#"
if [ "$1" = clone ]; then
  if [ ! -f "$ATTEMPTS" ]; then
    printf first > "$ATTEMPTS"
    mkdir -p "$5/.git"
    printf partial > "$5/partial"
    printf 'fatal: could not resolve host: github.com\n' >&2
    exit 128
  fi
  test ! -e "$5/partial" || exit 19
  mkdir -p "$5/.git"
fi
exit 0
"#,
    );
    let env = tools
        .env()
        .with_var("ATTEMPTS", attempts.display().to_string());
    let cancel = CancellationToken::new();
    Checkout {
        dir: &dir,
        env: &env,
        cancel: &cancel,
    }
    .sync("https://github.com/o/r.git", &mut |_, _| {})
    .await
    .unwrap();
    assert!(dir.join(".git").is_dir());
    assert!(!dir.join("partial").exists());
}

const FLAKY_GIT: &str = r#"
if [ "$3" = checkout ]; then
    count=0
    if [ -f "$ATTEMPTS" ]; then read -r count < "$ATTEMPTS"; fi
    count=$((count + 1))
    printf '%s\n' "$count" > "$ATTEMPTS"
    if [ "$count" -le "$FAILURES" ]; then
        printf '%s\n' 'error: RPC failed; curl 18 Transferred a partial file' >&2
        printf '%s\n' 'fatal: early EOF' >&2
        printf '%s\n' 'fatal: could not fetch blob from promisor remote' >&2
        exit 128
    fi
fi
exec "$REAL_GIT" "$@"
"#;

#[tokio::test]
async fn checkout_recovers_from_interrupted_partial_clone_download() {
    // Given a real blobless clone and a transport interruption on the first checkout.
    let fixture = GitFixture::new();
    fixture.git(&["config", "uploadpack.allowFilter", "true"]);
    let sha = fixture.commit("payload.txt", "downloaded", "payload");
    let work = tempfile::tempdir().unwrap();
    let dir = work.path().join("checkout");
    let attempts = work.path().join("attempts");
    let real_git = std::process::Command::new("/bin/sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(real_git.status.success());
    let tools = FakeTools::new().tool("git", FLAKY_GIT);
    let env = tools
        .env()
        .with_var(
            "REAL_GIT",
            String::from_utf8(real_git.stdout).unwrap().trim(),
        )
        .with_var("ATTEMPTS", attempts.display().to_string())
        .with_var("FAILURES", "1")
        .with_var("GIT_CONFIG_GLOBAL", "/dev/null")
        .with_var("GIT_CONFIG_NOSYSTEM", "1");
    let cancel = CancellationToken::new();
    let checkout = Checkout {
        dir: &dir,
        env: &env,
        cancel: &cancel,
    };
    checkout.sync(&fixture.url(), &mut |_, _| {}).await.unwrap();
    std::fs::create_dir_all(dir.join("target")).unwrap();
    std::fs::write(dir.join("target/cache"), "warm").unwrap();

    // When checkout has to recover before the exact revision can be built.
    let actual = checkout.checkout(&sha, &mut |_, _| {}).await.unwrap();

    // Then the requested revision is materialized without discarding build caches.
    assert_eq!(actual, sha);
    assert_eq!(
        std::fs::read_to_string(dir.join("payload.txt")).unwrap(),
        "downloaded"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("target/cache")).unwrap(),
        "warm"
    );
    assert_eq!(std::fs::read_to_string(attempts).unwrap().trim(), "2");
}
