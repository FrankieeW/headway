use super::*;

use crate::shell::ShellEnv;

fn failure(line: &str) -> Error {
    Error::CommandFailed {
        command: "git checkout".into(),
        status: Some(128),
        tail: vec![line.into()],
    }
}

#[test]
fn retries_transport_interruptions_only() {
    for line in [
        "error: RPC failed; curl 18 Transferred a partial file",
        "error: RPC failed; curl 56 Recv failure: Connection reset by peer",
        "error: RPC failed; curl 92 HTTP/2 stream was not closed cleanly",
        "fatal: early EOF",
        "fetch-pack: unexpected disconnect while reading sideband packet",
    ] {
        assert!(interrupted_transfer(&failure(line)), "{line}");
    }
    for line in [
        "fatal: Authentication failed",
        "error: RPC failed; curl 22 The requested URL returned error: 403",
        "fatal: repository not found",
        "fatal: reference is not a tree: missing",
        "fatal: could not fetch blob from promisor remote",
        "fatal: Unable to create index.lock: File exists",
        "error: unable to write file: No space left on device",
    ] {
        assert!(!interrupted_transfer(&failure(line)), "{line}");
    }
    assert!(!interrupted_transfer(&Error::Cancelled));
}

fn failing_command(attempts: &std::path::Path, line: &str) -> Cmd {
    Cmd::new("/bin/sh", &ShellEnv::current())
        .args([
            "-c",
            "printf x >> \"$ATTEMPTS\"; printf '%s\\n' \"$FAILURE\" >&2; exit 128",
        ])
        .env("ATTEMPTS", attempts.display().to_string())
        .env("FAILURE", line)
}

#[tokio::test]
async fn stops_after_three_attempts_and_preserves_last_error() {
    let work = tempfile::tempdir().unwrap();
    let attempts = work.path().join("attempts");
    let cmd = failing_command(&attempts, "error: RPC failed; curl 18 partial file");
    let error = run(&cmd, &CancellationToken::new(), &mut |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(std::fs::read_to_string(attempts).unwrap(), "xxx");
    let Error::CommandFailed {
        command,
        status,
        tail,
    } = error
    else {
        panic!("expected original command failure");
    };
    assert_eq!(command, cmd.display());
    assert_eq!(status, Some(128));
    assert_eq!(tail, ["error: RPC failed; curl 18 partial file"]);
}

#[tokio::test]
async fn permanent_failure_runs_once() {
    let work = tempfile::tempdir().unwrap();
    let attempts = work.path().join("attempts");
    let cmd = failing_command(&attempts, "fatal: Authentication failed");
    let error = run(&cmd, &CancellationToken::new(), &mut |_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(error, Error::CommandFailed { .. }));
    assert_eq!(std::fs::read_to_string(attempts).unwrap(), "x");
}

#[tokio::test]
async fn cancellation_during_backoff_stops_retrying() {
    let work = tempfile::tempdir().unwrap();
    let attempts = work.path().join("attempts");
    let cmd = failing_command(&attempts, "fatal: early EOF");
    let cancel = CancellationToken::new();
    let error = run(&cmd, &cancel, &mut |stream, _| {
        if stream == LogStream::Info {
            cancel.cancel();
        }
    })
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled));
    assert_eq!(std::fs::read_to_string(attempts).unwrap(), "x");
}

#[tokio::test]
async fn cancelled_operation_does_not_spawn_git() {
    let work = tempfile::tempdir().unwrap();
    let attempts = work.path().join("attempts");
    let cmd = failing_command(&attempts, "fatal: early EOF");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = run(&cmd, &cancel, &mut |_, _| {}).await.unwrap_err();
    assert!(matches!(error, Error::Cancelled));
    assert!(!attempts.exists());
}
