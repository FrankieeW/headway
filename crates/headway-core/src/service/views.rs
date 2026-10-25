use super::*;

/// One app as the CLI and the UI show it.
#[derive(Debug, Clone, Serialize)]
pub struct AppView {
    pub app: TrackedApp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation: Option<super::InstallationDetection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<crate::github::PullRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<InstalledRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<CheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<LastError>,
    /// An update is running right now (in any process).
    pub busy: bool,
    pub auto_update_paused: bool,
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
    pub(super) fn new(app: TrackedApp, mut state: AppState, busy: bool) -> Self {
        use super::presence::{self, Presence};
        let installation = match super::installation::detect(&app, &state) {
            Ok(detection) => Some(detection),
            Err(error) => {
                state.last_error = Some(LastError::from_error(&error, "check"));
                None
            }
        };
        let observed_status = match installation.as_ref().map(|detection| detection.presence) {
            Some(Presence::Missing) => {
                state.installed = None;
                Some((AppStatus::NotInstalled, UpdateStatus::NotInstalled))
            }
            Some(Presence::Present) if !presence::has_baseline(&app, state.installed.as_ref()) => {
                state.installed = None;
                Some((AppStatus::Unknown, UpdateStatus::Unknown))
            }
            Some(Presence::Invalid) => {
                state.installed = None;
                Some((AppStatus::Unknown, UpdateStatus::Unknown))
            }
            Some(Presence::Present | Presence::Unknown) | None => None,
        };
        if let (Some((_, status)), Some(check)) = (observed_status, state.last_check.as_mut()) {
            check.status = status;
            check.pending_total = 0;
        }
        let status = if busy {
            AppStatus::Busy
        } else if state.last_error.is_some() {
            AppStatus::Error
        } else if let Some((status, _)) = observed_status {
            status
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
            installation,
            pull_request: state.pull_request.filter(|_| state.installed.is_some()),
            installed: state.installed,
            last_check: state.last_check,
            last_error: state.last_error,
            busy,
            auto_update_paused: state.auto_update_paused,
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
