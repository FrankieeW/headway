use super::*;

/// One app as the CLI and the UI show it.
#[derive(Debug, Clone, Serialize)]
pub struct AppView {
    pub app: TrackedApp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<InstalledRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<CheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<LastError>,
    /// An update is running right now (in any process).
    pub busy: bool,
    pub status: AppStatus,
    /// Newer commits/releases known from the last check.
    pub pending_count: u32,
}

/// A single status for badges and scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Unknown,
    Unchecked,
    UpToDate,
    UpdateAvailable,
    NotInstalled,
    Diverged,
    Error,
    Busy,
}

impl AppView {
    pub(super) fn new(app: TrackedApp, state: AppState, busy: bool) -> Self {
        let status = if busy {
            AppStatus::Busy
        } else if state.last_error.is_some() {
            AppStatus::Error
        } else {
            match state.last_check.as_ref().map(|check| check.status) {
                None => AppStatus::Unchecked,
                Some(UpdateStatus::UpToDate) => AppStatus::UpToDate,
                Some(UpdateStatus::UpdateAvailable) => AppStatus::UpdateAvailable,
                Some(UpdateStatus::NotInstalled) => AppStatus::NotInstalled,
                Some(UpdateStatus::Diverged) => AppStatus::Diverged,
                Some(UpdateStatus::Unknown) => AppStatus::Unknown,
            }
        };
        let pending_count = state
            .last_check
            .as_ref()
            .filter(|check| {
                matches!(
                    check.status,
                    UpdateStatus::UpdateAvailable | UpdateStatus::Diverged
                )
            })
            .map_or(0, |check| check.pending_total);
        Self {
            app,
            installed: state.installed,
            last_check: state.last_check,
            last_error: state.last_error,
            busy,
            status,
            pending_count,
        }
    }

    pub const fn has_update(&self) -> bool {
        self.pending_count > 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenStatus {
    pub source: TokenSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<RateLimit>,
    /// Why the token could not be verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemovedApp {
    pub id: String,
    pub purged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogFile {
    pub path: String,
    pub content: String,
}
