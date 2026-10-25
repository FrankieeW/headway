//! Desktop consumes shared restart requests only after admission closes new runs.
use std::time::Duration;

use crate::state::{self, AppState, CmdResult, IntoCmd as _};
use headway_core::SelfUpdateStatus;
use tauri::{AppHandle, Manager as _, Runtime, State};

/// How often a pending restart request is checked.
const POLL: Duration = Duration::from_secs(2);

#[tauri::command]
pub async fn self_update_status(state: State<'_, AppState>) -> CmdResult<SelfUpdateStatus> {
    state.hw.self_update_status().await.cmd()
}

#[tauri::command]
pub async fn request_self_restart(state: State<'_, AppState>) -> CmdResult<SelfUpdateStatus> {
    state.hw.request_self_restart().await.cmd()
}

fn updates_idle(state: &AppState) -> headway_core::Result<bool> {
    Ok(state.active_runs() == 0 && !state.hw.list()?.iter().any(|app| app.busy))
}

pub fn start<R: Runtime>(app: AppHandle<R>) {
    state::supervise("self update", app, |app| async move {
        let state = app.state::<AppState>();
        if let Err(error) = state.hw.confirm_self_restart() {
            eprintln!("headway: restart confirmation: {error}");
        }
        let primary = std::env::var_os("HEADWAY_VARIANT_ID").is_none()
            && !state.hw.running_is_variant().unwrap_or(true);
        match if primary {
            state.hw.cli_status(None).await.map(Some)
        } else {
            Ok(None)
        } {
            Ok(Some(status)) if status.needs_repair && status.points_to.is_some() => {
                if let Err(error) = state.hw.fix_cli_link(None).await {
                    eprintln!("headway: CLI link repair: {error}");
                }
            }
            Ok(_) => {}
            Err(error) => eprintln!("headway: CLI link status: {error}"),
        }
        loop {
            tokio::time::sleep(POLL).await;
            // Cheap gate: only build the whole app list when a request can
            // actually be consumed by this installation.
            match state.hw.self_restart_pending() {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    eprintln!("headway: restart request check: {error}");
                    continue;
                }
            }
            match updates_idle(&state) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    eprintln!("headway: restart idle check: {error}");
                    continue;
                }
            }
            match state
                .hw
                .take_self_restart_when(|| state.try_admit_restart_if_idle())
            {
                Ok(true) => app.restart(),
                Ok(_) => {}
                Err(error) => {
                    state.cancel_restart_admission();
                    eprintln!("headway: restart request: {error}");
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "self_update_tests.rs"]
mod tests;
