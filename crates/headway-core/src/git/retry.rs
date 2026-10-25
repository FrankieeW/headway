//! Bounded retries for replayable Git operations that may download objects.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::events::LogStream;
use crate::process::{self, Cmd};

pub(super) async fn run(
    cmd: &Cmd,
    cancel: &CancellationToken,
    on_line: &mut (dyn FnMut(LogStream, &str) + Send),
) -> Result<()> {
    run_inner(cmd, cancel, on_line, None).await
}

pub(super) async fn clone(
    cmd: &Cmd,
    dir: &std::path::Path,
    cancel: &CancellationToken,
    on_line: &mut (dyn FnMut(LogStream, &str) + Send),
) -> Result<()> {
    if !dir.is_absolute()
        || dir.parent().is_none()
        || dir.file_name().is_none()
        || dir
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(Error::InvalidInput(
            "clone destination must be an owned absolute child path".into(),
        ));
    }
    run_inner(cmd, cancel, on_line, Some(dir)).await
}

async fn cleanup(dir: &std::path::Path) -> Result<()> {
    let dir = dir.to_owned();
    tokio::task::spawn_blocking(move || crate::install::remove_any(&dir))
        .await
        .map_err(|error| Error::io("clone cleanup worker", std::io::Error::other(error)))?
}

async fn run_inner(
    cmd: &Cmd,
    cancel: &CancellationToken,
    on_line: &mut (dyn FnMut(LogStream, &str) + Send),
    clone_dir: Option<&std::path::Path>,
) -> Result<()> {
    // Git's transport diagnostics must be stable regardless of the user's locale.
    let cmd = cmd.clone().env("LC_ALL", "C");
    let mut attempt = 1;
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(dir) = clone_dir {
            cleanup(dir).await?;
        }
        let result = process::run(&cmd, cancel, on_line).await;
        if result.is_err()
            && let Some(dir) = clone_dir
        {
            cleanup(dir).await?;
        }
        match result {
            Err(error) if attempt < 3 && interrupted_transfer(&error) => {
                let delay = Duration::from_secs(attempt);
                attempt += 1;
                on_line(
                    LogStream::Info,
                    &format!(
                        "Git transfer interrupted; retrying attempt {attempt}/3 in {}s",
                        delay.as_secs()
                    ),
                );
                tokio::select! {
                    () = cancel.cancelled() => return Err(Error::Cancelled),
                    () = tokio::time::sleep(delay) => {}
                }
            }
            result => return result,
        }
    }
}

fn interrupted_transfer(error: &Error) -> bool {
    let Error::CommandFailed { tail, .. } = error else {
        return false;
    };
    tail.iter().any(|line| {
        line.contains("fatal: early EOF")
            || line.contains("unexpected disconnect while reading sideband packet")
            || line.to_ascii_lowercase().contains("could not resolve host")
            || line.to_ascii_lowercase().contains("failed to connect")
            || ["6", "7", "16", "18", "28", "52", "55", "56", "92"]
                .iter()
                .any(|code| line.contains(&format!("RPC failed; curl {code} ")))
    })
}

#[cfg(test)]
mod tests;
