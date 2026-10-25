//! Reading versions off the system: run a command, pick the version out of its output.

use crate::error::{Error, Result};
use crate::process::{self, Captured, Cmd};
use crate::shell::ShellEnv;
use tokio_util::sync::CancellationToken;

/// Output lines kept when a probe fails.
const TAIL_LINES: usize = 15;
/// Per-stream cap for probe output, matching the process layer's raw-capture budget: npm and
/// Homebrew registry inventories travel through probes and can be large. Overflow is an
/// explicit error, never truncated data.
const PROBE_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Probe wall-clock cap; applied with `Cmd::with_timeout_cap`, so a shorter caller deadline
/// (e.g. a 15 s bundle probe) is preserved instead of being replaced by this default.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// `command` run with `/bin/sh -c` in the home directory.
pub fn shell(env: &ShellEnv, command: &str) -> Cmd {
    let cmd = Cmd::new("/bin/sh", env).args(["-c", command]);
    match dirs::home_dir() {
        Some(home) => cmd.cwd(home),
        None => cmd,
    }
}

/// Runs `cmd` and returns stdout and stderr, failing on a non-zero exit.
pub async fn output(cancel: &CancellationToken, cmd: &Cmd) -> Result<Captured> {
    let out = capture(cancel, cmd).await?;
    if out.success() {
        Ok(out)
    } else {
        Err(failed(cmd, &out))
    }
}

/// Bounds short version/metadata queries, which may invoke a network client.
///
/// Delegates to the process layer's bounded, cancellation-aware capture: each stream is capped
/// at [`PROBE_MAX_BYTES`] (the same 16 MiB budget as raw process capture) with an explicit
/// overflow error — never a silently truncated "valid" document — and the child's process group
/// is killed on cancel/timeout/overflow.
///
/// The wall clock is `min(cmd.timeout, PROBE_TIMEOUT)` via [`Cmd::with_timeout_cap`]: a caller's
/// shorter deadline (e.g. a 15 s bundle probe) is preserved, never replaced by the probe default.
pub async fn capture(cancel: &CancellationToken, cmd: &Cmd) -> Result<Captured> {
    let bounded = cmd.clone().with_timeout_cap(PROBE_TIMEOUT);
    capture_with_limit(cancel, &bounded, PROBE_MAX_BYTES).await
}

/// Test seam: identical bounded capture with an explicit per-stream budget (e.g. 1 KiB).
async fn capture_with_limit(cancel: &CancellationToken, cmd: &Cmd, limit: u64) -> Result<Captured> {
    process::capture_with_cancel(cancel, cmd, limit).await
}

pub fn failed(cmd: &Cmd, out: &Captured) -> Error {
    let lines: Vec<String> = out
        .stdout
        .lines()
        .chain(out.stderr.lines())
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let skip = lines.len().saturating_sub(TAIL_LINES);
    Error::CommandFailed {
        command: cmd.display(),
        status: out.status,
        tail: lines.into_iter().skip(skip).collect(),
    }
}

/// The first version-looking token: `claude 2.0.31 (Claude Code)` → `2.0.31`,
/// `rustc 1.99.1 (58cf8f9c3 2026-10-13)` → `1.99.1`, `v22.11.0` → `22.11.0`.
pub fn version_in(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || ",;()[]{}\"'<>=".contains(c))
        .map(|token| {
            token
                .trim_start_matches(['v', 'V'])
                .trim_end_matches(['.', ':'])
        })
        .find(|token| is_version(token))
        .map(str::to_owned)
}

fn is_version(token: &str) -> bool {
    let core = token.split(['-', '+', '_']).next().unwrap_or_default();
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() >= 2
        && numbers
            .iter()
            .all(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+_".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_versions_in_tool_output() {
        let cases = [
            ("claude 2.0.31 (Claude Code)", Some("2.0.31")),
            ("rustc 1.99.1 (58cf8f9c3 2026-10-13)", Some("1.99.1")),
            ("v22.11.0", Some("22.11.0")),
            ("git version 2.56.0", Some("2.56.0")),
            ("Version: 1.2.3-beta.4+build", Some("1.2.3-beta.4+build")),
            ("ripgrep 14.1.1_1", Some("14.1.1_1")),
            ("{\"tag_name\":\"v0.9.2\"}", Some("0.9.2")),
            ("no version here 2026", None),
            ("", None),
        ];
        for (text, expected) in cases {
            assert_eq!(version_in(text).as_deref(), expected, "{text}");
        }
    }

    #[tokio::test]
    async fn shell_commands_report_failures_with_output() {
        let env = ShellEnv::current();
        let out = output(&CancellationToken::new(), &shell(&env, "echo 1.2.3"))
            .await
            .unwrap();
        assert_eq!(version_in(&out.stdout).as_deref(), Some("1.2.3"));
        let err = output(
            &CancellationToken::new(),
            &shell(&env, "echo nope >&2; exit 4"),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, Error::CommandFailed { status: Some(4), tail, .. } if tail == &["nope"])
        );
    }
    #[tokio::test]
    async fn oversized_multiline_output_is_an_explicit_error_not_truncated_data() {
        let env = ShellEnv::current();
        // Endless multi-line output: the byte budget must fail, not accumulate forever.
        let flood = shell(&env, "/usr/bin/yes x");
        let err = capture_with_limit(&CancellationToken::new(), &flood, 1024)
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("exceeded"), "{err}");
    }

    #[tokio::test]
    async fn oversized_single_line_is_an_explicit_error_not_truncated_data() {
        let env = ShellEnv::current();
        // One endless line with no newline: the byte budget must still fire.
        let line = shell(&env, "/usr/bin/yes x | /usr/bin/tr -d '\\n'");
        let err = capture_with_limit(&CancellationToken::new(), &line, 1024)
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("exceeded"), "{err}");
    }

    #[tokio::test]
    async fn large_but_allowed_output_is_captured_completely() {
        let env = ShellEnv::current();
        // 2 MiB is above a 1 MiB guess but under the 16 MiB probe budget: complete success.
        let out = capture(
            &CancellationToken::new(),
            &shell(&env, "/usr/bin/yes x | /usr/bin/head -c 2097152"),
        )
        .await
        .unwrap();
        assert!(out.success());
        assert_eq!(out.stdout.len(), 2 * 1024 * 1024);
    }

    #[tokio::test]
    async fn cancellation_terminates_the_probe_group_quickly() {
        let env = ShellEnv::current();
        let cmd = shell(&env, "exec /bin/sleep 30");
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            token.cancel();
        });
        let started = std::time::Instant::now();
        let err = capture(&cancel, &cmd).await.unwrap_err();
        assert!(matches!(err, Error::Cancelled), "{err}");
        // The group is killed on cancel; without that this would wait the full 30 s sleep.
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[tokio::test]
    async fn probe_timeout_kills_the_group_and_does_not_wait_for_the_child() {
        let env = ShellEnv::current();
        let cmd = shell(&env, "exec /bin/sleep 30").timeout(std::time::Duration::from_millis(150));
        let started = std::time::Instant::now();
        let err = capture(&CancellationToken::new(), &cmd).await.unwrap_err();
        assert!(
            matches!(&err, Error::CommandFailed { tail, .. } if tail.iter().any(|line| line.contains("timed out"))),
            "{err}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[tokio::test]
    async fn caller_shorter_timeout_is_preserved_by_probe_capture() {
        let env = ShellEnv::current();
        // Regression: probe capture must not replace a caller's 50 ms deadline with its 60 s cap.
        let cmd = shell(&env, "exec /bin/sleep 30").timeout(std::time::Duration::from_millis(50));
        let started = std::time::Instant::now();
        let err = capture(&CancellationToken::new(), &cmd).await.unwrap_err();
        assert!(
            matches!(&err, Error::CommandFailed { tail, .. } if tail.iter().any(|line| line.contains("timed out"))),
            "{err}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "caller timeout was replaced by the probe cap: {:?}",
            started.elapsed()
        );
    }
}
