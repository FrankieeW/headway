//! Headway engine: follow GitHub repositories by commit or by release and keep
//! the apps and CLI tools built from them installed and current on macOS.
//!
//! [`Headway`] is the entry point shared by the `headway` CLI and the desktop app.

pub mod archive;
pub mod asset;
pub mod check;
pub mod config;
pub mod doctor;
pub mod error;
pub mod events;
pub mod git;
pub mod github;
pub mod inspect;
pub mod install;
pub mod lock;
pub mod paths;
pub mod presets;
pub mod process;
pub mod repo;
pub mod service;
pub mod shell;
pub mod state;
pub mod store;
pub mod token;
pub mod update;
pub mod version;

#[cfg(test)]
mod testing;

pub use config::{
    AppKind, AssetInstall, Config, Install, Settings, SourceInstall, Track, TrackedApp,
};
pub use error::{Error, ErrorInfo, Result};
pub use events::{EventSink, LogStream, NullSink, Stage, UpdateEvent};
pub use service::{AppStatus, AppView, EnvMode, Headway, LogFile, RemovedApp, TokenStatus};
pub use state::{Candidate, CheckResult, HistoryEntry, InstalledRef, UpdateStatus};
pub use tokio_util::sync::CancellationToken;
pub use update::{Target, UpdateOutcome, UpdatePlan};
