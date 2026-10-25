//! Notices when `config.toml` / `state.json` change underneath the app (the CLI,
//! an agent, or a hand edit) and tells the window to refetch.
//!
//! This is a fingerprint poll, not a file-system watch: two `stat` calls per tick
//! and no extra dependency. The tick is fast while the window is open or an
//! update runs, and slow while the app idles in the menu bar, where an external
//! edit can wait a few seconds without anyone noticing.

use std::time::Duration;

use tauri::{AppHandle, Manager as _, Runtime};

use crate::state::{self, AppState, apps_changed};

/// Client-visible work (an open window, a running update): keep it snappy.
const ACTIVE_POLL: Duration = Duration::from_secs(2);
/// Menu-bar idle: cheaper ticks; the window refetches when it is shown again.
const IDLE_POLL: Duration = Duration::from_secs(10);

pub fn start<R: Runtime>(app: AppHandle<R>) {
    state::supervise("watcher", app, |app| async move {
        let mut seen = app.state::<AppState>().hw.store().fingerprint();
        loop {
            tokio::time::sleep(poll_interval(&app)).await;
            let now = app.state::<AppState>().hw.store().fingerprint();
            if now != seen {
                seen = now;
                apps_changed(&app);
            }
        }
    });
}

fn poll_interval<R: Runtime>(app: &AppHandle<R>) -> Duration {
    let running = app.state::<AppState>().active_runs() > 0;
    let visible = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(true);
    idle_poll(running, visible)
}

const fn idle_poll(running: bool, visible: bool) -> Duration {
    if running || visible {
        ACTIVE_POLL
    } else {
        IDLE_POLL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_menu_bar_polls_less_often() {
        assert_eq!(idle_poll(true, false), ACTIVE_POLL);
        assert_eq!(idle_poll(false, true), ACTIVE_POLL);
        assert_eq!(idle_poll(false, false), IDLE_POLL);
        assert!(IDLE_POLL >= ACTIVE_POLL * 2);
    }
}
