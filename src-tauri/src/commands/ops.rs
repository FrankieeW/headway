//! Checks, updates and baselines.

use headway_core::{
    AppView, CheckSummary, Error, ErrorInfo, LogFile, RevisionMethod, Target, UpdatePlan,
    UpdateRequest,
};
use tauri::{AppHandle, Runtime, State};

use crate::runner::{self, Origin};
use crate::state::{AppState, CmdResult, IntoCmd as _, apps_changed, invalid};

#[tauri::command]
pub async fn check_app<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<AppView> {
    let result = state.hw.check(&id).await;
    // A failed check is recorded on the app, so the list changes either way.
    apps_changed(&app);
    result.cmd()
}

/// Checks every tracked app. The record shape is core's, so the desktop and the
/// CLI print the same document (`checked`, `with_updates`, `failed`, `results`).
#[tauri::command]
pub async fn check_all<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<CheckSummary> {
    let summary = state.hw.check_summary(&[]).await.cmd()?;
    apps_changed(&app);
    Ok(summary)
}

/// Plans an update. `method` forces the install method for this run only.
#[tauri::command]
pub async fn plan_update(
    state: State<'_, AppState>,
    id: String,
    target: Target,
    method: Option<RevisionMethod>,
) -> CmdResult<UpdatePlan> {
    state
        .hw
        .plan_with(
            &id,
            &UpdateRequest {
                target,
                method: method.unwrap_or(RevisionMethod::Configured),
            },
        )
        .await
        .cmd()
}

/// Starts an update in the background; progress arrives as events.
///
/// The run is admitted synchronously: a refused start (another process updating,
/// the app quitting, or a restart already admitted) is reported here instead of
/// leaving the window with a run that never emits events.
/// `method` forces the install method for this run only.
#[tauri::command]
pub async fn start_update<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    target: Target,
    method: Option<RevisionMethod>,
) -> CmdResult<()> {
    if state.hw.get(&id).cmd()?.busy {
        return Err(ErrorInfo::from(&Error::Busy(id)));
    }
    runner::spawn_update(
        &app,
        id,
        target,
        method.unwrap_or(RevisionMethod::Configured),
        Origin::User,
    )
}

#[tauri::command]
pub async fn cancel_update(state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    if state.cancel_run(&id) {
        Ok(true)
    } else if state.hw.get(&id).cmd()?.busy {
        Err(invalid(format!(
            "`{id}` is being updated by another process (the CLI); stop it there"
        )))
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn mark_installed<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    reference: String,
) -> CmdResult<AppView> {
    let view = state.hw.mark(&id, &reference).await.cmd()?;
    apps_changed(&app);
    Ok(view)
}

#[tauri::command]
pub async fn read_log(state: State<'_, AppState>, id: String) -> CmdResult<Option<LogFile>> {
    state.hw.last_log(&id).cmd()
}
