use super::shell_quote;
use crate::error::{Error, Result};
use crate::shell::ShellEnv;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// A command with an explicit, complete environment.
#[derive(Debug, Clone)]
pub struct Cmd {
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: BTreeMap<String, String>,
    pub(super) timeout: std::time::Duration,
}

impl Cmd {
    /// Starts a user-requested application without holding an update job open.
    pub fn spawn_detached(&self) -> Result<u32> {
        let mut command = self.command();
        command
            .kill_on_drop(false)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn().map_err(|error| self.spawn_error(error))?;
        child.id().ok_or_else(|| {
            Error::io(
                "launch application",
                std::io::Error::other("process has no id"),
            )
        })
    }

    /// No subprocess inherits any token source accepted by Headway. Git receives
    /// explicitly scoped `GIT_CONFIG_*` credentials instead.
    pub fn new(program: impl Into<String>, env: &ShellEnv) -> Self {
        let mut vars = env.vars().clone();
        for key in [crate::token::TOKEN_ENV, "GITHUB_TOKEN", "GH_TOKEN"] {
            vars.remove(key);
        }
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: vars,
            timeout: std::time::Duration::from_secs(10 * 60),
        }
    }

    #[must_use]
    pub fn timeout(self, timeout: std::time::Duration) -> Self {
        Self { timeout, ..self }
    }

    #[must_use]
    pub(crate) fn with_timeout_cap(self, limit: std::time::Duration) -> Self {
        let timeout = self.timeout.min(limit);
        Self { timeout, ..self }
    }

    /// For filesystem workers and Drop safety nets, never on an async worker.
    /// Uses the same sanitized environment, deadline and process-group isolation.
    pub(crate) fn run_blocking(&self) -> Result<()> {
        use std::os::unix::process::CommandExt as _;
        let mut command = std::process::Command::new(&self.program);
        command
            .args(&self.args)
            .env_clear()
            .envs(&self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        let mut child = command.spawn().map_err(|error| self.spawn_error(error))?;
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = child.try_wait().map_err(|error| self.spawn_error(error))? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(Error::CommandFailed {
                        command: self.display(),
                        status: status.code(),
                        tail: vec![],
                    })
                };
            }
            if start.elapsed() >= self.timeout {
                super::signal_group(child.id(), "KILL");
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::CommandFailed {
                    command: self.display(),
                    status: None,
                    tail: vec![format!(
                        "command timed out after {} seconds",
                        self.timeout.as_secs()
                    )],
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
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

    pub(super) fn command(&self) -> Command {
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

    pub(super) fn spawn_error(&self, err: std::io::Error) -> Error {
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
