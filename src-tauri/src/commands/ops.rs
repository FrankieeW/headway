//! Checks, updates and baselines.

use headway_core::{AppView, LogFile, Target, UpdatePlan};
use serde::Serialize;
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

#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    id: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckAllSummary {
    checked: usize,
    with_updates: usize,
    failed: Vec<Failure>,
}

#[tauri::command]
pub async fn check_all<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<CheckAllSummary> {
    let results = state.hw.check_many(&[]).await.cmd()?;
    apps_changed(&app);
    let with_updates = results
        .iter()
        .filter(|(_, result)| result.as_ref().is_ok_and(AppView::has_update))
        .count();
    let failed = results
        .iter()
        .filter_map(|(id, result)| {
            result.as_ref().err().map(|err| Failure {
                id: id.clone(),
                message: err.to_string(),
            })
        })
        .collect();
    Ok(CheckAllSummary {
        checked: results.len(),
        with_updates,
        failed,
    })
}

#[tauri::command]
pub async fn plan_update(
    state: State<'_, AppState>,
    id: String,
    target: Target,
) -> CmdResult<UpdatePlan> {
    state.hw.plan(&id, &target).await.cmd()
}

/// Starts an update in the background; progress arrives as events.
#[tauri::command]
pub async fn start_update<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    target: Target,
) -> CmdResult<()> {
    let view = state.hw.get(&id).cmd()?;
    if view.busy {
        return Err(headway_core::ErrorInfo::from(&headway_core::Error::Busy(
            id,
        )));
    }
    runner::spawn_update(&app, id, target, Origin::User);
    Ok(())
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
