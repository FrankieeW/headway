//! Reading versions off the system: run a command, pick the version out of its output.

use crate::error::{Error, Result};
use crate::process::{self, Captured, Cmd};
use crate::shell::ShellEnv;
use tokio_util::sync::CancellationToken;

/// Output lines kept when a probe fails.
const TAIL_LINES: usize = 15;

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
pub async fn capture(cancel: &CancellationToken, cmd: &Cmd) -> Result<Captured> {
    capture_with_timeout(cancel, cmd, std::time::Duration::from_secs(60)).await
}

async fn capture_with_timeout(
    cancel: &CancellationToken,
    cmd: &Cmd,
    timeout: std::time::Duration,
) -> Result<Captured> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let probe_cancel = cancel.child_token();
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut on_line = |stream, line: &str| {
        let output = match stream {
            crate::events::LogStream::Stdout => &mut stdout,
            crate::events::LogStream::Stderr | crate::events::LogStream::Info => &mut stderr,
        };
        output.push_str(line);
        output.push('\n');
    };
    let result = {
        let run = process::run(cmd, &probe_cancel, &mut on_line);
        tokio::pin!(run);
        tokio::select! {
            result = &mut run => result,
            () = tokio::time::sleep(timeout) => {
                probe_cancel.cancel();
                match run.await {
                    Ok(()) | Err(Error::Cancelled) => {},
                    Err(err) => return Err(err),
                }
                return Err(Error::CommandFailed { command: cmd.display(), status: None, tail: vec![format!("version query timed out after {} seconds", timeout.as_secs())] });
            }
        }
    };
    let status = match result {
        Ok(()) => Some(0),
        Err(Error::CommandFailed { status, .. }) => status,
        Err(err) => return Err(err),
    };
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    Ok(Captured {
        status,
        stdout,
        stderr,
    })
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
}
