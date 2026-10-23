//! Update progress on stderr.

use std::io::{IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};

use headway_core::{EventSink, LogStream, UpdateEvent};

use crate::human::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Silent,
    /// Stage lines and Headway's own notes (default for `--json`/`--ai`).
    Stages,
    /// Everything, including raw build output (default for humans).
    Full,
}

pub struct TerminalSink {
    level: LogLevel,
    palette: Palette,
    tty: bool,
    /// A `\r` progress line is on screen and needs a newline before other output.
    progress_open: AtomicBool,
}

impl TerminalSink {
    pub fn new(level: LogLevel, color: bool) -> Self {
        let tty = std::io::stderr().is_terminal();
        Self {
            level,
            palette: Palette {
                color: color && tty,
            },
            tty,
            progress_open: AtomicBool::new(false),
        }
    }

    fn close_progress(&self) {
        if self.progress_open.swap(false, Ordering::Relaxed) {
            eprintln!();
        }
    }
}

impl EventSink for TerminalSink {
    fn emit(&self, event: UpdateEvent) {
        let p = self.palette;
        match (self.level, event) {
            (LogLevel::Silent, _) => {}
            (
                _,
                UpdateEvent::Stage {
                    app_id, message, ..
                },
            ) => {
                self.close_progress();
                eprintln!("{} {} {message}", p.cyan("==>"), p.bold(&app_id));
            }
            (
                _,
                UpdateEvent::Log {
                    stream: LogStream::Info,
                    line,
                    ..
                },
            ) => {
                self.close_progress();
                eprintln!("    {}", p.cyan(&line));
            }
            (LogLevel::Full, UpdateEvent::Log { line, .. }) => {
                self.close_progress();
                eprintln!("    {}", p.dim(&line));
            }
            (_, UpdateEvent::Progress { done, total, .. }) if self.tty => {
                let text = match total {
                    Some(total) if total > 0 => {
                        let percent = done.saturating_mul(100) / total;
                        format!("{percent:>3}%  {} / {}", mb(done), mb(total))
                    }
                    _ => mb(done),
                };
                eprint!("\r    {text}   ");
                let _ = std::io::stderr().flush();
                self.progress_open.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

fn mb(bytes: u64) -> String {
    let tenths = bytes / (1024 * 1024 / 10);
    format!("{}.{} MB", tenths / 10, tenths % 10)
}
