//! State shared by commands, the tray, the scheduler and the watcher.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use headway_core::{CancellationToken, ErrorInfo, Headway};
use serde::Serialize;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};

pub const APPS_CHANGED: &str = "headway://apps-changed";
pub const UPDATE_EVENT: &str = "headway://update-event";
pub const UPDATE_FINISHED: &str = "headway://update-finished";
pub const NAVIGATE: &str = "headway://navigate";

pub struct AppState {
    pub hw: Arc<Headway>,
    /// Updates started by this process, by app id, so they can be cancelled.
    runs: Mutex<HashMap<String, CancellationToken>>,
    /// Wakes the scheduler after settings change.
    pub wake: Arc<tokio::sync::Notify>,
    /// "Check now" from the tray: run a check cycle on the next wake-up.
    check_requested: AtomicBool,
}

impl AppState {
    pub fn new(hw: Headway) -> Self {
        Self {
            hw: Arc::new(hw),
            runs: Mutex::new(HashMap::new()),
            wake: Arc::new(tokio::sync::Notify::new()),
            check_requested: AtomicBool::new(false),
        }
    }

    /// Asks the scheduler for an immediate check cycle.
    pub fn wake_check(&self) {
        self.check_requested.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub fn take_check_request(&self) -> bool {
        self.check_requested.swap(false, Ordering::SeqCst)
    }

    /// Registers a run; `None` if this app is already updating here.
    pub fn begin_run(&self, id: &str) -> Option<CancellationToken> {
        let mut runs = self.runs.lock().ok()?;
        if runs.contains_key(id) {
            return None;
        }
        let token = CancellationToken::new();
        runs.insert(id.to_owned(), token.clone());
        Some(token)
    }

    pub fn end_run(&self, id: &str) {
        if let Ok(mut runs) = self.runs.lock() {
            runs.remove(id);
        }
    }

    pub fn cancel_run(&self, id: &str) -> bool {
        self.runs
            .lock()
            .ok()
            .and_then(|runs| runs.get(id).cloned())
            .is_some_and(|token| {
                token.cancel();
                true
            })
    }
}

/// Command error: the same shape the CLI prints (`code`, `message`, `hint`, `details`).
pub type CmdResult<T> = Result<T, ErrorInfo>;

pub trait IntoCmd<T> {
    fn cmd(self) -> CmdResult<T>;
}

impl<T> IntoCmd<T> for headway_core::Result<T> {
    fn cmd(self) -> CmdResult<T> {
        self.map_err(|err| ErrorInfo::from(&err))
    }
}

pub fn invalid(message: impl Into<String>) -> ErrorInfo {
    ErrorInfo::from(&headway_core::Error::InvalidInput(message.into()))
}

/// Tells the window to refetch and refreshes the tray.
pub fn apps_changed<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(APPS_CHANGED, ());
    crate::tray::refresh(app);
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigatePayload {
    pub app_id: String,
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    set_dock_visible(app, true);
}

/// Menu-bar-only while the window is hidden, a normal Dock app while it is shown.
pub fn set_dock_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if visible {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}
