//! Headway engine: follow GitHub repositories by commit or by release and keep
//! the apps and CLI tools built from them installed and current on macOS.
//!
//! [`Headway`] is the entry point shared by the `headway` CLI and the desktop app.

pub mod archive;
pub mod asset;
pub mod brew;
pub mod build_detection;
pub mod build_info;
pub mod check;
pub mod cli_link;
pub mod commit_metadata;
pub mod config;
pub mod doctor;
pub mod error;
pub mod events;
pub mod git;
pub mod github;
pub mod inspect;
pub mod install;
pub mod inventory;
pub mod lock;
pub mod npm;
pub mod paths;
pub mod policy;
pub mod presets;
mod probe;
pub mod process;
pub mod recipes;
pub mod recovery;
pub mod repo;
mod repository_family;
pub mod revision_history;
pub mod revision_install;
pub mod script;
mod self_subscription;
pub mod self_update;
pub mod service;
pub mod shell;
pub mod source;
pub mod source_options;
pub mod source_storage;
pub mod state;
pub mod store;
pub mod token;
pub mod update;
pub mod version;

#[cfg(test)]
mod testing;

pub use build_detection::{BuildCandidate, BuildCandidateSource};
pub use build_info::BuildInfo;
pub use commit_metadata::{CommitCiState, CommitMetadata, CommitVersionSource};
pub use config::{
    AppKind, AssetInstall, Config, Install, Settings, SourceInstall, Track, TrackedApp,
};
pub use error::{Error, ErrorInfo, Result};
pub use events::{EventSink, LogStream, NullSink, Stage, UpdateEvent};
pub use github::PullRequest;
pub use policy::{AutoUpdateMode, UpdatePolicy};
pub use revision_history::CommitPage;
pub use revision_install::{
    LaunchOutcome, RevisionMethod, RevisionOptions, RevisionRequest, VariantRequest,
};
pub use service::{AppStatus, AppView, EnvMode, Headway, LogFile, RemovedApp, TokenStatus};
pub use service::{
    AutoUpdateReport, CheckFailure, CheckLatest, CheckRecord, CheckSummary, PullRequestPage,
};
pub use service::{InstallationDetection, Presence};
pub use source::{BrewSource, GithubSource, NpmSource, ScriptSource, Source, SourceSpec};
pub use source_options::{GithubOptions, RepositoryOption, RepositorySettings, VariantInfo};
pub use state::{Candidate, CheckResult, HistoryEntry, InstalledRef, UpdateStatus};
pub use tokio_util::sync::CancellationToken;
pub use update::{Target, UpdateOutcome, UpdatePlan};

pub use recovery::{BackupEntry, RestoreOutcome};

pub use cli_link::CliStatus;
pub use inventory::{InventoryCandidate, InventoryError, InventoryReport};
pub use recipes::AppRecipe;
pub use self_update::SelfUpdateStatus;

pub use source_storage::{SourceStorage, SourceStorageStatus};
