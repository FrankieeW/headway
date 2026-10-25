//! State shared by commands, the tray, the scheduler and the watcher.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use headway_core::{CancellationToken, ErrorInfo, Headway};
use serde::Serialize;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};

use crate::host::Host;

pub const APPS_CHANGED: &str = "headway://apps-changed";
pub const UPDATE_EVENT: &str = "headway://update-event";
pub const UPDATE_FINISHED: &str = "headway://update-finished";
pub const NAVIGATE: &str = "headway://navigate";

/// How long quitting waits for cancelled updates to stop.
const EXIT_GRACE: Duration = Duration::from_secs(10);
/// How often the exit drain re-checks for finished runs.
const EXIT_POLL: Duration = Duration::from_millis(100);

pub struct AppState {
    pub hw: Arc<Headway>,
    /// Updates started by this process, by app id, so they can be cancelled.
    runs: Mutex<HashMap<String, CancellationToken>>,
    /// Wakes the scheduler after settings change.
    pub wake: Arc<tokio::sync::Notify>,
    /// "Check now" from the tray: run a check cycle on the next wake-up.
    check_requested: AtomicBool,
    /// Quitting: no new runs are admitted once this is set.
    exiting: AtomicBool,
    /// Seals update admission once an idle restart has been admitted.
    restarting: AtomicBool,
    /// The exit drain ran out of grace: the next exit request must not be delayed.
    exit_forced: AtomicBool,
    /// Exactly one drain task waits for the runs of this process.
    draining: AtomicBool,
    /// Host services (opener, login item); installed during setup or by tests.
    host: OnceLock<Arc<dyn Host>>,
}

impl AppState {
    pub fn new(hw: Headway) -> Self {
        Self {
            hw: Arc::new(hw),
            runs: Mutex::new(HashMap::new()),
            wake: Arc::new(tokio::sync::Notify::new()),
            check_requested: AtomicBool::new(false),
            exiting: AtomicBool::new(false),
            restarting: AtomicBool::new(false),
            exit_forced: AtomicBool::new(false),
            draining: AtomicBool::new(false),
            host: OnceLock::new(),
        }
    }

    /// Cancels every update this process runs; returns how many there were.
    pub fn cancel_all(&self) -> usize {
        self.runs.lock().map_or(0, |runs| {
            runs.values().for_each(CancellationToken::cancel);
            runs.len()
        })
    }

    pub fn active_runs(&self) -> usize {
        self.runs.lock().map_or(0, |runs| runs.len())
    }

    /// Marks the app as quitting; returns whether it already was.
    pub fn begin_exit(&self) -> bool {
        self.exiting.swap(true, Ordering::SeqCst)
    }

    /// Claims the single drain task; false when one already waits.
    pub fn begin_drain(&self) -> bool {
        !self.draining.swap(true, Ordering::SeqCst)
    }

    /// The drain gave up waiting: let the process exit even with runs left.
    pub fn force_exit(&self) {
        self.exit_forced.store(true, Ordering::SeqCst);
    }

    fn exit_forced(&self) -> bool {
        self.exit_forced.load(Ordering::SeqCst)
    }

    /// Asks the scheduler for an immediate check cycle.
    pub fn wake_check(&self) {
        self.check_requested.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub fn take_check_request(&self) -> bool {
        self.check_requested.swap(false, Ordering::SeqCst)
    }

    /// Registers a run; `None` if this app is already updating here or the app is
    /// quitting/restarting. Callers must turn `None` into the stable `busy` error.
    pub fn begin_run(&self, id: &str) -> Option<CancellationToken> {
        let mut runs = match self.runs.lock() {
            Ok(runs) => runs,
            Err(poisoned) => poisoned.into_inner(),
        };
        if self.exiting.load(Ordering::SeqCst)
            || self.restarting.load(Ordering::SeqCst)
            || runs.contains_key(id)
        {
            return None;
        }
        let token = CancellationToken::new();
        runs.insert(id.to_owned(), token.clone());
        Some(token)
    }

    /// Admits a restart only while idle and prevents new updates from racing it.
    pub fn try_admit_restart_if_idle(&self) -> bool {
        let Ok(runs) = self.runs.lock() else {
            return false;
        };
        if !runs.is_empty() || self.exiting.load(Ordering::SeqCst) {
            return false;
        }
        self.restarting.store(true, Ordering::SeqCst);
        true
    }

    /// Reopens admission if persisting an admitted restart failed.
    pub fn cancel_restart_admission(&self) {
        if let Ok(_runs) = self.runs.lock() {
            self.restarting.store(false, Ordering::SeqCst);
        }
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

    /// Installs the host services once; later attempts are ignored.
    pub fn install_host(&self, host: Arc<dyn Host>) {
        let _ = self.host.set(host);
    }

    /// The host services; only missing if the app was built without setup.
    pub fn host(&self) -> CmdResult<Arc<dyn Host>> {
        self.host
            .get()
            .cloned()
            .ok_or_else(|| invalid("host services are not ready"))
    }
}

/// Registers a run for `id` or reports the stable `busy` error. Both update
/// runners admit through here, so a refused start is never silently accepted.
pub fn admit_run(state: &AppState, id: &str) -> CmdResult<CancellationToken> {
    state
        .begin_run(id)
        .ok_or_else(|| ErrorInfo::from(&headway_core::Error::Busy(id.to_owned())))
}

/// What an exit request should do. Split out from the Tauri event handler so the
/// lifecycle is testable without exiting a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDisposition {
    /// Nothing is running: let the process exit.
    Exit,
    /// Runs are still being cancelled: block the exit. `start_drain` is true for
    /// the request that owns the drain task.
    Block { start_drain: bool },
}

/// Cancels this process's runs and decides whether the exit may proceed. Every
/// exit request goes through here, so a second quit never exits prematurely with
/// children still alive; the drain forces the exit after its grace period.
pub fn prepare_exit(state: &AppState) -> ExitDisposition {
    if state.exit_forced() {
        return ExitDisposition::Exit;
    }
    state.begin_exit();
    state.cancel_all();
    if state.active_runs() == 0 {
        return ExitDisposition::Exit;
    }
    ExitDisposition::Block {
        start_drain: state.begin_drain(),
    }
}

/// Graceful quit: cancel this process's runs and exit once they are gone.
pub fn request_quit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    state.begin_exit();
    state.cancel_all();
    if state.active_runs() == 0 {
        app.exit(0);
        return;
    }
    if state.begin_drain() {
        spawn_exit_drain(app.clone());
    }
}

/// Waits for cancelled runs to stop, then exits. The grace bound keeps a wedged
/// child from preventing the quit forever; the next exit request is never delayed.
pub fn spawn_exit_drain<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let deadline = Instant::now() + EXIT_GRACE;
        while app.state::<AppState>().active_runs() > 0 && Instant::now() < deadline {
            tokio::time::sleep(EXIT_POLL).await;
        }
        let state = app.state::<AppState>();
        state.force_exit();
        app.exit(0);
    });
}

/// Runs a long-lived background component. A panicked task is logged and
/// restarted instead of silently disappearing for the lifetime of the process.
pub fn supervise<R, F, Fut>(name: &'static str, app: AppHandle<R>, run: F)
where
    R: Runtime,
    F: Fn(AppHandle<R>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        loop {
            let task = tauri::async_runtime::spawn(run(app.clone()));
            match task.await {
                Ok(()) => {
                    eprintln!("headway: {name} stopped");
                    return;
                }
                Err(error) => {
                    eprintln!("headway: {name} failed ({error}); restarting in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });
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
