use crate::error::{Error, Result};
use crate::events::{EventSink, LogStream, Stage, UpdateEvent};
use chrono::Utc;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Log files kept per app.
pub const LOG_KEEP: usize = 20;

/// The log of one run: written to a file and mirrored to the event sink.
pub struct RunLog<'a> {
    app_id: String,
    path: PathBuf,
    file: Mutex<File>,
    sink: &'a dyn EventSink,
    last_progress: Mutex<Option<Instant>>,
}

impl<'a> RunLog<'a> {
    pub fn create(dir: &Path, app_id: &str, sink: &'a dyn EventSink) -> Result<Self> {
        fs::create_dir_all(dir).map_err(|err| Error::io_at("create", dir, err))?;
        let path = dir.join(format!("{}.log", Utc::now().format("%Y%m%d-%H%M%S%.3f")));
        let file = File::create(&path).map_err(|err| Error::io_at("create", &path, err))?;
        prune_logs(dir, LOG_KEEP);
        Ok(Self {
            app_id: app_id.to_owned(),
            path,
            file: Mutex::new(file),
            sink,
            last_progress: Mutex::new(None),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn line(&self, stream: LogStream, text: &str) {
        self.write(stream, text);
        self.sink.emit(UpdateEvent::Log {
            app_id: self.app_id.clone(),
            stream,
            line: text.to_owned(),
        });
    }

    pub fn info(&self, text: &str) {
        self.line(LogStream::Info, text);
    }

    /// Starts a stage. The message goes to the file and, once, to the sink as a stage event.
    pub fn stage(&self, stage: Stage, message: impl Into<String>) {
        let message = message.into();
        self.write(LogStream::Info, &message);
        self.sink.emit(UpdateEvent::Stage {
            app_id: self.app_id.clone(),
            stage,
            message,
        });
    }

    fn write(&self, stream: LogStream, text: &str) {
        if let Ok(mut file) = self.file.lock() {
            let tag = match stream {
                LogStream::Stdout => "out",
                LogStream::Stderr => "err",
                LogStream::Info => "headway",
            };
            let _ = writeln!(file, "{} [{tag}] {text}", Utc::now().format("%H:%M:%S"));
        }
    }

    /// Reports download progress, at most every 200 ms (and always at the end).
    pub fn progress(&self, done: u64, total: Option<u64>) {
        let finished = total.is_some_and(|total| done >= total);
        if let Ok(mut last) = self.last_progress.lock() {
            let due = last.is_none_or(|at| at.elapsed() >= Duration::from_millis(200));
            if !(due || finished) {
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

/// Deletes all but the newest `keep` log files in `dir`.
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

/// The newest log file in `dir`.
pub fn latest_log(dir: &Path) -> Option<PathBuf> {
    log_files(dir).into_iter().max()
}
