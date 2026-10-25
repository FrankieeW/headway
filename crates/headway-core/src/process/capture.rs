use super::{Captured, Cmd, terminate};
use crate::error::{Error, Result};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio_util::sync::CancellationToken;

pub(super) async fn capture(cmd: &Cmd) -> Result<Captured> {
    // Long-lived package-manager inventories can exceed the small probe budget.
    capture_with_cancel(&CancellationToken::new(), cmd, 16 * 1024 * 1024).await
}

/// Raw bounded capture: overflow never returns a truncated successful document.
pub(crate) async fn capture_with_cancel(
    cancel: &CancellationToken,
    cmd: &Cmd,
    limit: u64,
) -> Result<Captured> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let mut child = cmd
        .command()
        .spawn()
        .map_err(|error| cmd.spawn_error(error))?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let deadline = cmd.timeout.min(Duration::from_secs(60));
    let work = async {
        let (status, stdout, stderr) = tokio::try_join!(
            async {
                child
                    .wait()
                    .await
                    .map_err(|error| Error::io("wait for query", error))
            },
            read(stdout, limit),
            read(stderr, limit)
        )?;
        Ok(Captured {
            status: status.code(),
            stdout,
            stderr,
        })
    };
    let result = tokio::select! {
        result = tokio::time::timeout(deadline, work) => result.unwrap_or_else(|_| {
            Err(Error::CommandFailed { command: cmd.display(), status: None,
                tail: vec![format!("query timed out after {} seconds", deadline.as_secs())] })
        }),
        () = cancel.cancelled() => Err(Error::Cancelled),
    };
    if result.is_err() {
        if let Some(pid) = pid {
            super::signal_group(pid, "KILL");
        }
        terminate(&mut child).await;
    }
    result
}

async fn read(reader: Option<impl AsyncRead + Unpin>, limit: u64) -> Result<String> {
    let Some(reader) = reader else {
        return Ok(String::new());
    };
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| Error::io("read command output", error))?;
    if bytes.len() > usize::try_from(limit).unwrap_or(usize::MAX) {
        return Err(Error::InvalidInput(format!(
            "query output exceeded {limit} bytes"
        )));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
