//! Running external commands (git, build scripts, archive tools) with streamed,
//! cancellable output.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::events::LogStream;
use crate::shell::ShellEnv;

/// Output lines kept for [`Error::CommandFailed::tail`].
const TAIL_LINES: usize = 30;
/// How long output is still collected once the command itself has exited. Background
/// children (`daemon &`) may hold the pipes open indefinitely; they are not waited for.
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);

/// A command with an explicit, complete environment.
#[derive(Debug, Clone)]
pub struct Cmd {
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: BTreeMap<String, String>,
}

impl Cmd {
    /// A command running in `env`, minus Headway's own token variable: no subprocess
    /// needs it (git authenticates through `GIT_CONFIG_*`), and builds must not see it.
    pub fn new(program: impl Into<String>, env: &ShellEnv) -> Self {
        let mut vars = env.vars().clone();
        vars.remove(crate::token::TOKEN_ENV);
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: vars,
        }
    }

    #[must_use]
    pub fn arg(self, arg: impl Into<String>) -> Self {
        let mut args = self.args;
        args.push(arg.into());
        Self { args, ..self }
    }

    #[must_use]
    pub fn args<I, S>(self, extra: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        extra.into_iter().fold(self, Self::arg)
    }

    #[must_use]
    pub fn cwd(self, dir: impl Into<PathBuf>) -> Self {
        Self {
            cwd: Some(dir.into()),
            ..self
        }
    }

    #[must_use]
    pub fn env(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let mut env = self.env;
        env.insert(key.into(), value.into());
        Self { env, ..self }
    }

    /// The command line as a user could paste it into a shell. Never contains secrets:
    /// credentials are passed through the environment, which is not displayed.
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .env_clear()
            .envs(&self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Own process group, so cancelling reaches grandchildren (cargo → rustc …).
            .process_group(0);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command
    }

    fn spawn_error(&self, err: std::io::Error) -> Error {
        let context = if err.kind() == std::io::ErrorKind::NotFound {
            format!(
                "start `{}` (not found on PATH or missing working directory)",
                self.program
            )
        } else {
            format!("start `{}`", self.program)
        };
        Error::io(context, err)
    }
}

/// Quotes `arg` for a POSIX shell when it contains anything beyond safe characters.
pub fn shell_quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@+,%^".contains(c));
    if safe {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[derive(Debug, Clone)]
pub struct Captured {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Captured {
    pub const fn success(&self) -> bool {
        matches!(self.status, Some(0))
    }
}

/// Runs `cmd` to completion and captures its output (for short queries).
pub async fn capture(cmd: &Cmd) -> Result<Captured> {
    let output = cmd
        .command()
        .output()
        .await
        .map_err(|err| cmd.spawn_error(err))?;
    Ok(Captured {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Runs `cmd`, handing every output line to `on_line`.
///
/// Fails with [`Error::CommandFailed`] on a non-zero exit and with
/// [`Error::Cancelled`] (after terminating the whole process group) when `cancel` fires.
pub async fn run(
    cmd: &Cmd,
    cancel: &CancellationToken,
    on_line: &mut (dyn FnMut(LogStream, &str) + Send),
) -> Result<()> {
    let mut child = cmd.command().spawn().map_err(|err| cmd.spawn_error(err))?;
    let (tx, mut rx) = mpsc::channel::<(LogStream, String)>(256);
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(forward_lines(stdout, LogStream::Stdout, tx.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(forward_lines(stderr, LogStream::Stderr, tx.clone()));
    }
    drop(tx);

    let mut tail: VecDeque<String> = VecDeque::with_capacity(TAIL_LINES);
    let wait_error = |err| Error::io(format!("wait for `{}`", cmd.display()), err);
    // Until the command exits, forwarding its output as it comes.
    let status = loop {
        tokio::select! {
            message = rx.recv() => match message {
                Some((stream, line)) => record(&mut tail, on_line, stream, &line),
                // Output closed first: just wait for the exit.
                None => break tokio::select! {
                    status = child.wait() => status.map_err(wait_error)?,
                    () = cancel.cancelled() => {
                        terminate(&mut child).await;
                        return Err(Error::Cancelled);
                    }
                },
            },
            status = child.wait() => break status.map_err(wait_error)?,
            () = cancel.cancelled() => {
                terminate(&mut child).await;
                return Err(Error::Cancelled);
            }
        }
    };
    // Collect what is still in flight, without waiting for background children.
    let drain = tokio::time::sleep(DRAIN_AFTER_EXIT);
    tokio::pin!(drain);
    loop {
        tokio::select! {
            message = rx.recv() => match message {
                Some((stream, line)) => record(&mut tail, on_line, stream, &line),
                None => break,
            },
            () = &mut drain => break,
        }
    }
    if status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailed {
            command: cmd.display(),
            status: status.code(),
            tail: tail.into(),
        })
    }
}

fn record(
    tail: &mut VecDeque<String>,
    on_line: &mut (dyn FnMut(LogStream, &str) + Send),
    stream: LogStream,
    line: &str,
) {
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line.to_owned());
    on_line(stream, line);
}

async fn forward_lines(
    reader: impl AsyncRead + Unpin,
    stream: LogStream,
    tx: mpsc::Sender<(LogStream, String)>,
) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                // Progress bars redraw with `\r`; keep only the final state of the line.
                let line = text
                    .trim_end_matches(['\n', '\r'])
                    .rsplit('\r')
                    .next()
                    .unwrap_or_default();
                if tx.send((stream, strip_ansi(line))).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn terminate(child: &mut Child) {
    if let Some(pid) = child.id() {
        signal_group(pid, "TERM");
        if tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_ok()
        {
            return;
        }
        signal_group(pid, "KILL");
    }
    let _ = child.kill().await;
}

fn signal_group(pid: u32, signal: &str) {
    let _ = std::process::Command::new("/bin/kill")
        .args(["-s", signal, "--", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Removes ANSI CSI/OSC escape sequences (colors, cursor movement, hyperlinks).
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::*;

    fn sh(script: &str) -> Cmd {
        Cmd::new("/bin/sh", &ShellEnv::current()).args(["-c", script])
    }

    #[tokio::test]
    async fn streams_both_streams_and_succeeds() {
        let mut lines = Vec::new();
        run(
            &sh("echo out; echo err 1>&2; printf 'no newline'"),
            &CancellationToken::new(),
            &mut |stream, line| lines.push((stream, line.to_owned())),
        )
        .await
        .unwrap();
        assert!(lines.contains(&(LogStream::Stdout, "out".into())));
        assert!(lines.contains(&(LogStream::Stderr, "err".into())));
        assert!(lines.contains(&(LogStream::Stdout, "no newline".into())));
    }

    #[tokio::test]
    async fn failure_reports_status_and_tail() {
        let err = run(
            &sh("echo first; echo boom 1>&2; exit 3"),
            &CancellationToken::new(),
            &mut |_, _| {},
        )
        .await
        .unwrap_err();
        match err {
            Error::CommandFailed {
                status,
                tail,
                command,
            } => {
                assert_eq!(status, Some(3));
                assert_eq!(tail.last().map(String::as_str), Some("boom"));
                assert!(command.starts_with("/bin/sh -c"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_kills_the_process_group() {
        let cancel = CancellationToken::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let trigger = cancel.clone();
        let started = std::time::Instant::now();
        let err = run(
            &sh("echo started; sleep 30 & wait"),
            &cancel,
            &mut |_, line| {
                seen.lock().unwrap().push(line.to_owned());
                trigger.cancel();
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(
            seen.lock().unwrap().first().map(String::as_str),
            Some("started")
        );
    }

    #[tokio::test]
    async fn background_children_holding_the_output_are_not_waited_for() {
        let started = std::time::Instant::now();
        let mut lines = Vec::new();
        run(
            &sh("echo before; sleep 20 & echo after"),
            &CancellationToken::new(),
            &mut |_, line| lines.push(line.to_owned()),
        )
        .await
        .unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "took {:?}",
            started.elapsed()
        );
        assert_eq!(lines, ["before", "after"]);
    }

    #[tokio::test]
    async fn missing_program_is_an_io_error() {
        let err = capture(&Cmd::new("/nonexistent/tool", &ShellEnv::current()))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "io_error");
        assert!(err.to_string().contains("/nonexistent/tool"));
    }

    #[tokio::test]
    async fn capture_collects_output_and_env() {
        let out = capture(&sh("printf \"$GREETING\"").env("GREETING", "hi"))
            .await
            .unwrap();
        assert!(out.success());
        assert_eq!(out.stdout, "hi");
    }

    #[tokio::test]
    async fn headway_token_is_not_passed_to_subprocesses() {
        let env = ShellEnv::from_vars([
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            (crate::token::TOKEN_ENV.to_owned(), "ghp_secret".to_owned()),
            ("OTHER".to_owned(), "kept".to_owned()),
        ]);
        let script = "printf '%s|%s' \"${HEADWAY_GITHUB_TOKEN:-unset}\" \"$OTHER\"";
        let out = capture(&Cmd::new("/bin/sh", &env).args(["-c", script]))
            .await
            .unwrap();
        assert_eq!(out.stdout, "unset|kept");
    }

    #[test]
    fn display_quotes_only_when_needed() {
        let cmd =
            Cmd::new("git", &ShellEnv::default()).args(["commit", "-m", "it's done", "--x=1"]);
        assert_eq!(cmd.display(), r"git commit -m 'it'\''s done' --x=1");
    }

    #[test]
    fn strips_ansi_sequences() {
        assert_eq!(
            strip_ansi("\u{1b}[1;32mCompiling\u{1b}[0m foo"),
            "Compiling foo"
        );
        assert_eq!(
            strip_ansi("\u{1b}]8;;https://x.dev\u{7}link\u{1b}]8;;\u{7}"),
            "link"
        );
        assert_eq!(strip_ansi("plain"), "plain");
    }
}
