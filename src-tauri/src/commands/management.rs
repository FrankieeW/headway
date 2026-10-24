//! Subscription management shared with the CLI.
use headway_core::{AppRecipe, AppView, BackupEntry, InventoryReport, RestoreOutcome, TrackedApp};
use tauri::{AppHandle, Runtime, State};

use crate::state::{AppState, CmdResult, IntoCmd as _, apps_changed};

#[tauri::command]
pub async fn discover_apps(
    state: State<'_, AppState>,
    sources: Option<Vec<String>>,
) -> CmdResult<InventoryReport> {
    state
        .hw
        .discover_apps(&sources.unwrap_or_default())
        .await
        .cmd()
}

#[tauri::command]
pub async fn import_apps<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    apps: Vec<TrackedApp>,
) -> CmdResult<Vec<AppView>> {
    let views = state.hw.import_apps(apps).cmd()?;
    apps_changed(&app);
    Ok(views)
}

#[tauri::command]
pub async fn list_recipes(state: State<'_, AppState>) -> CmdResult<Vec<AppRecipe>> {
    Ok(state.hw.recipes())
}

#[tauri::command]
pub async fn resume_auto_update<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<AppView> {
    let view = state.hw.resume_auto_update(&id).cmd()?;
    state.wake_check();
    apps_changed(&app);
    Ok(view)
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>, id: String) -> CmdResult<Vec<BackupEntry>> {
    state.hw.list_backups(&id).cmd()
}

struct RestoreGuard<'a> {
    state: &'a AppState,
    id: &'a str,
}
impl Drop for RestoreGuard<'_> {
    fn drop(&mut self) {
        self.state.end_run(self.id);
    }
}

#[tauri::command]
pub async fn restore_backup<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    backup_id: String,
) -> CmdResult<RestoreOutcome> {
    let cancel = state
        .begin_run(&id)
        .ok_or_else(|| headway_core::ErrorInfo::from(&headway_core::Error::Busy(id.clone())))?;
    let _guard = RestoreGuard {
        state: &state,
        id: &id,
    };
    apps_changed(&app);
    let sink = crate::runner::WindowSink { app: app.clone() };
    let result = state
        .hw
        .restore_backup(&id, &backup_id, &sink, &cancel)
        .await
        .cmd();
    apps_changed(&app);
    result
}

#[tauri::command]
pub async fn fix_cli_link(
    state: State<'_, AppState>,
    app_path: Option<String>,
) -> CmdResult<headway_core::CliStatus> {
    state.hw.fix_cli_link(app_path.as_deref()).await.cmd()
}

#[tauri::command]
pub async fn ensure_self_subscription<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<AppView> {
    let view = state.hw.ensure_self_subscription().await.cmd()?;
    apps_changed(&app);
    Ok(view)
}

#[tauri::command]
pub async fn commit_metadata(
    state: State<'_, AppState>,
    id: String,
    sha: String,
    refresh: Option<bool>,
) -> CmdResult<headway_core::CommitMetadata> {
    state
        .hw
        .commit_metadata(&id, &sha, refresh.unwrap_or(false))
        .await
        .cmd()
}
