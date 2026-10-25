//! Byte-counting sink for archive preflight: never buffer decoded payloads.
use super::Cmd;
use crate::error::{Error, Result};
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio_util::sync::CancellationToken;

pub(crate) async fn measure(
    cmd: &Cmd,
    cancel: &CancellationToken,
    max_bytes: u64,
    max_lines: Option<u64>,
) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let mut child = cmd
        .command()
        .spawn()
        .map_err(|error| cmd.spawn_error(error))?;
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::InvalidInput("preflight has no output pipe".into()))?;
    let stderr = child.stderr.take();
    // Bound the diagnostics independently and keep draining the pipe.
    let diagnostics = tokio::spawn(async move {
        let Some(mut stderr) = stderr else {
            return;
        };
        let mut bytes = [0; 8192];
        while stderr.read(&mut bytes).await.is_ok_and(|read| read > 0) {}
    });
    let work = async {
        let ((), status) = tokio::try_join!(count(stdout, max_bytes, max_lines), async {
            child
                .wait()
                .await
                .map_err(|error| Error::io("wait for archive preflight", error))
        })?;
        if !status.success() {
            return Err(Error::CommandFailed {
                command: cmd.display(),
                status: status.code(),
                tail: vec!["archive preflight failed; archive may be corrupt or unsafe".into()],
            });
        }
        Ok(())
    };
    let result = tokio::select! {
        result = tokio::time::timeout(cmd.timeout, work) => result.unwrap_or_else(|_| Err(Error::CommandFailed {
            command: cmd.display(), status: None, tail: vec!["archive preflight timed out".into()] })),
        () = cancel.cancelled() => Err(Error::Cancelled),
    };
    if result.is_err() {
        if let Some(pid) = pid {
            super::signal_group(pid, "KILL");
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    diagnostics.abort();
    result
}

async fn count(
    mut reader: impl AsyncRead + Unpin,
    max_bytes: u64,
    max_lines: Option<u64>,
) -> Result<()> {
    let mut bytes = vec![0; 16 * 1024];
    let mut total = 0_u64;
    let mut lines = 0_u64;
    let mut line_bytes = 0;
    loop {
        let read = reader
            .read(&mut bytes)
            .await
            .map_err(|error| Error::io("read decoded archive", error))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > max_bytes {
            return Err(Error::NoAsset(
                "archive exceeds the decompressed byte budget".into(),
            ));
        }
        if let Some(max_lines) = max_lines {
            for byte in &bytes[..read] {
                if *byte == b'\n' {
                    lines += 1;
                    line_bytes = 0;
                } else {
                    line_bytes += 1;
                }
                if lines > max_lines || line_bytes > 4096 {
                    return Err(Error::NoAsset(
                        "archive exceeds the entry-count/path-length budget".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}
