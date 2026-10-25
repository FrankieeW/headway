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
    // Git's transport diagnostics must be stable regardless of the user's locale.
    let cmd = cmd.clone().env("LC_ALL", "C");
    let mut attempt = 1;
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match process::run(&cmd, cancel, on_line).await {
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
            || ["16", "18", "28", "52", "55", "56", "92"]
                .iter()
                .any(|code| line.contains(&format!("RPC failed; curl {code} ")))
    })
}

#[cfg(test)]
mod tests;
