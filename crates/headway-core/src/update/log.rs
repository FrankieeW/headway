use crate::error::{Error, Result};
use crate::events::{EventSink, LogStream, Stage, UpdateEvent};
use chrono::Utc;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const LOG_KEEP: usize = 20;
const MAX_LOG_BYTES: usize = 16 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 10_000;
const MAX_LINE_BYTES: usize = 8 * 1024;

struct Output {
    file: File,
    bytes: usize,
    events: usize,
    event_bytes: usize,
    limited: bool,
    write_failed: bool,
    meaningful: bool,
}

/// Disk and event budgets are independent. A noisy build cannot fill either.
pub struct RunLog<'a> {
    app_id: String,
    path: PathBuf,
    output: Mutex<Output>,
    warnings: Mutex<Vec<String>>,
    sink: &'a dyn EventSink,
    last_progress: Mutex<Option<Instant>>,
}

fn bounded(text: &str) -> String {
    if text.len() <= MAX_LINE_BYTES {
        return text.to_owned();
    }
    let end = text
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= MAX_LINE_BYTES)
        .last()
        .unwrap_or(0);
    format!("{} … [line truncated]", &text[..end])
}

impl<'a> RunLog<'a> {
    pub fn create(dir: &Path, app_id: &str, sink: &'a dyn EventSink) -> Result<Self> {
        fs::create_dir_all(dir).map_err(|error| Error::io_at("create", dir, error))?;
        let path = dir.join(format!("{}.log", Utc::now().format("%Y%m%d-%H%M%S%.9f")));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| Error::io_at("create", &path, error))?;
        Ok(Self {
            app_id: app_id.to_owned(),
            path,
            output: Mutex::new(Output {
                file,
                bytes: 0,
                events: 0,
                event_bytes: 0,
                limited: false,
                write_failed: false,
                meaningful: false,
            }),
            warnings: Mutex::new(Vec::new()),
            sink,
            last_progress: Mutex::new(None),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn warnings(&self) -> Vec<String> {
        self.warnings
            .lock()
            .map(|warnings| warnings.clone())
            .unwrap_or_default()
    }
    pub fn warn(&self, text: &str) {
        let text = bounded(text);
        if let Ok(mut warnings) = self.warnings.lock()
            && warnings.len() < 32
            && !warnings.contains(&text)
        {
            warnings.push(text.clone());
        }
        self.line(LogStream::Info, &format!("WARNING: {text}"));
    }
    pub fn line(&self, stream: LogStream, text: &str) {
        let text = bounded(text);
        let mut emit = false;
        let mut notice = false;
        if let Ok(mut output) = self.output.lock() {
            output.meaningful = true;
            Self::write(&mut output, stream, &text);
            if stream == LogStream::Info
                || (output.events < MAX_EVENTS
                    && output.event_bytes + text.len() <= MAX_EVENT_BYTES)
            {
                output.events += 1;
                output.event_bytes += text.len();
                emit = true;
            } else if !output.limited {
                output.limited = true;
                notice = true;
            }
        }
        if emit {
            self.emit(stream, text);
        }
        if notice {
            self.emit(LogStream::Info, "Build output event limit reached; further output is suppressed (bounded disk log retained).".into());
        }
    }
    fn emit(&self, stream: LogStream, line: String) {
        self.sink.emit(UpdateEvent::Log {
            app_id: self.app_id.clone(),
            stream,
            line,
        });
    }
    pub fn info(&self, text: &str) {
        self.line(LogStream::Info, text);
    }
    pub fn stage(&self, stage: Stage, message: impl Into<String>) {
        let message = bounded(&message.into());
        if let Ok(mut output) = self.output.lock() {
            Self::write(&mut output, LogStream::Info, &message);
        }
        self.sink.emit(UpdateEvent::Stage {
            app_id: self.app_id.clone(),
            stage,
            message,
        });
    }
    fn write(output: &mut Output, stream: LogStream, text: &str) {
        if output.write_failed || output.bytes >= MAX_LOG_BYTES {
            return;
        }
        let tag = match stream {
            LogStream::Stdout => "out",
            LogStream::Stderr => "err",
            LogStream::Info => "headway",
        };
        let line = format!("{} [{tag}] {text}\n", Utc::now().format("%H:%M:%S"));
        let line = if output.bytes + line.len() + 100 > MAX_LOG_BYTES {
            output.bytes = MAX_LOG_BYTES;
            "[headway] Log byte limit reached; further output is suppressed.\n"
        } else {
            output.bytes += line.len();
            &line
        };
        if let Err(error) = output.file.write_all(line.as_bytes()) {
            output.write_failed = true;
            eprintln!("Headway could not write run log: {error}");
        }
    }
    pub fn progress(&self, done: u64, total: Option<u64>) {
        let finished = total.is_some_and(|total| done >= total);
        if let Ok(mut last) = self.last_progress.lock() {
            if !(last.is_none_or(|at| at.elapsed() >= Duration::from_millis(200)) || finished) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.sink.emit(UpdateEvent::Progress {
            app_id: self.app_id.clone(),
            done,
            total,
        });
    }
}

impl Drop for RunLog<'_> {
    fn drop(&mut self) {
        if let Ok(output) = self.output.lock() {
            if let Err(error) = output.file.sync_all() {
                eprintln!("Headway could not sync run log: {error}");
            }
            if output.meaningful
                && let Some(parent) = self.path.parent()
            {
                prune_logs(parent, LOG_KEEP);
            }
        }
    }
}

pub(super) fn prune_logs(dir: &Path, keep: usize) {
    let mut logs = log_files(dir);
    let excess = logs.len().saturating_sub(keep);
    logs.sort();
    for old in logs.into_iter().take(excess) {
        let _ = fs::remove_file(old);
    }
}
pub(super) fn log_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
                .collect()
        })
        .unwrap_or_default()
}
pub fn latest_log(dir: &Path) -> Option<PathBuf> {
    log_files(dir).into_iter().max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::UpdateEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Sink(AtomicUsize);
    impl EventSink for Sink {
        fn emit(&self, _: UpdateEvent) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[test]
    fn disk_and_event_output_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink(AtomicUsize::new(0));
        let log = RunLog::create(dir.path(), "tool", &sink).unwrap();
        for _ in 0..MAX_EVENTS * 2 {
            log.line(LogStream::Stdout, &"x".repeat(2000));
        }
        assert!(fs::metadata(log.path()).unwrap().len() <= MAX_LOG_BYTES as u64);
        assert!(sink.0.load(Ordering::Relaxed) <= MAX_EVENTS + 1);
        assert_eq!(
            fs::metadata(log.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    use std::os::unix::fs::PermissionsExt as _;
}
