use crate::events::LogStream;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::sync::mpsc;

pub(super) const MAX_LINE: usize = 8 * 1024;

/// Drain even discarded bytes, so a producer cannot block on a full pipe.
pub(super) async fn forward_lines(
    reader: impl AsyncRead + Unpin,
    stream: LogStream,
    tx: mpsc::Sender<(LogStream, String)>,
) {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::with_capacity(MAX_LINE);
    let mut truncated = false;
    loop {
        let available = match reader.fill_buf().await {
            Ok([]) | Err(_) => break,
            Ok(bytes) => bytes,
        };
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let keep = count.min(MAX_LINE.saturating_sub(line.len()));
        line.extend_from_slice(&available[..keep]);
        truncated |= keep < count;
        let complete = available[count - 1] == b'\n';
        reader.consume(count);
        if complete {
            if !send(&tx, stream, &line, truncated).await {
                return;
            }
            line.clear();
            truncated = false;
        }
    }
    if !line.is_empty() {
        let _ = send(&tx, stream, &line, truncated).await;
    }
}

async fn send(
    tx: &mpsc::Sender<(LogStream, String)>,
    stream: LogStream,
    bytes: &[u8],
    truncated: bool,
) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let text = text
        .trim_end_matches(['\n', '\r'])
        .rsplit('\r')
        .next()
        .unwrap_or_default();
    let mut line = super::strip_ansi(text);
    if truncated {
        line.push_str(" … [line truncated]");
    }
    tx.send((stream, line)).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn giant_lines_are_bounded_and_following_lines_survive() {
        let input = format!("{}\nend\n", "x".repeat(MAX_LINE * 100));
        let (tx, mut rx) = mpsc::channel(4);
        forward_lines(input.as_bytes(), LogStream::Stdout, tx).await;
        let first = rx.recv().await.unwrap().1;
        assert!(first.len() < MAX_LINE + 100);
        assert!(first.ends_with("[line truncated]"));
        assert_eq!(rx.recv().await.unwrap().1, "end");
    }
}
