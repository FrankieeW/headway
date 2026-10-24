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
