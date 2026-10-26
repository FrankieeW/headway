//! Tracked apps: list, add, edit, remove, inspect.

use headway_core::inspect::{PackageLookup, RepoInspection};
use headway_core::presets::{PRESETS, Preset};
use headway_core::{AppView, HistoryEntry, RemovedApp, SourceMatches, TrackedApp};
use tauri::{AppHandle, Runtime, State};

use crate::state::{AppState, CmdResult, IntoCmd as _, apps_changed};

#[tauri::command]
pub async fn list_apps(state: State<'_, AppState>) -> CmdResult<Vec<AppView>> {
    state.hw.list().cmd()
}

#[tauri::command]
pub async fn get_app(state: State<'_, AppState>, id: String) -> CmdResult<AppView> {
    state.hw.get(&id).cmd()
}

#[tauri::command]
pub async fn app_history(state: State<'_, AppState>, id: String) -> CmdResult<Vec<HistoryEntry>> {
    state.hw.history(&id).cmd()
}

#[tauri::command]
pub async fn add_app<R: Runtime>(
    handle: AppHandle<R>,
    state: State<'_, AppState>,
    app: TrackedApp,
) -> CmdResult<AppView> {
    let view = state.hw.add(app).cmd()?;
    apps_changed(&handle);
    Ok(view)
}

#[tauri::command]
pub async fn save_app<R: Runtime>(
    handle: AppHandle<R>,
    state: State<'_, AppState>,
    app: TrackedApp,
) -> CmdResult<AppView> {
    let view = state.hw.replace(app).cmd()?;
    apps_changed(&handle);
    Ok(view)
}

#[tauri::command]
pub async fn remove_app<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    purge: bool,
) -> CmdResult<RemovedApp> {
    let removed = state.hw.remove(&id, purge).cmd()?;
    apps_changed(&app);
    Ok(removed)
}

#[tauri::command]
pub async fn inspect_repo(state: State<'_, AppState>, repo: String) -> CmdResult<RepoInspection> {
    state.hw.inspect(&repo).await.cmd()
}

#[tauri::command]
pub async fn lookup_source(state: State<'_, AppState>, input: String) -> CmdResult<PackageLookup> {
    state.hw.lookup(&input).await.cmd()
}

#[tauri::command]
pub async fn list_presets() -> CmdResult<Vec<Preset>> {
    Ok(PRESETS.to_vec())
}

#[tauri::command]
pub async fn match_source(
    state: State<'_, AppState>,
    input: String,
    offline: Option<bool>,
) -> CmdResult<SourceMatches> {
    state
        .hw
        .match_source(&input, offline.unwrap_or(false))
        .await
        .cmd()
}
