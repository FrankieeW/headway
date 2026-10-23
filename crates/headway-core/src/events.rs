//! Progress events emitted while an update runs (rendered by the CLI, forwarded
//! to the window by the desktop app).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Resolve,
    Fetch,
    Checkout,
    Build,
    Download,
    Verify,
    Unpack,
    Install,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
    /// Lines written by Headway itself (commands it runs, decisions it makes).
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateEvent {
    Stage {
        app_id: String,
        stage: Stage,
        message: String,
    },
    Log {
        app_id: String,
        stream: LogStream,
        line: String,
    },
    Progress {
        app_id: String,
        done: u64,
        total: Option<u64>,
    },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: UpdateEvent);
}

/// Discards every event.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: UpdateEvent) {}
}

impl<F: Fn(UpdateEvent) + Send + Sync> EventSink for F {
    fn emit(&self, event: UpdateEvent) {
        self(event);
    }
}
