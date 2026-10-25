//! Settings, the GitHub token, diagnostics, the CLI link and launch at login.

use headway_core::TokenStatus;
use headway_core::config::Settings;
use headway_core::doctor::DoctorReport;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::cli_link::{self, CliStatus};
use crate::state::{AppState, CmdResult, IntoCmd as _, apps_changed, invalid};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    state.hw.settings().cmd()
}

#[tauri::command]
pub async fn save_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let saved = state.hw.save_settings(&settings).cmd()?;
    state.wake.notify_one();
    apps_changed(&app);
    Ok(saved)
}

#[tauri::command]
pub async fn token_status(state: State<'_, AppState>) -> CmdResult<TokenStatus> {
    state.hw.token_status().await.cmd()
}

#[tauri::command]
pub async fn set_token(state: State<'_, AppState>, token: String) -> CmdResult<TokenStatus> {
    state.hw.set_token(&token, true).await.cmd()
}

#[tauri::command]
pub async fn clear_token(state: State<'_, AppState>) -> CmdResult<TokenStatus> {
    state.hw.clear_token().await.cmd()?;
    state.hw.token_status().await.cmd()
}

#[tauri::command]
pub async fn doctor(state: State<'_, AppState>) -> CmdResult<DoctorReport> {
    state.hw.reload_env().await;
    Ok(state.hw.doctor().await)
}

#[derive(Debug, Clone, Serialize)]
pub struct PathsInfo {
    config_file: String,
    state_file: String,
    cache_dir: String,
    log_dir: String,
}

#[tauri::command]
pub async fn get_paths(state: State<'_, AppState>) -> CmdResult<PathsInfo> {
    let paths = state.hw.paths();
    Ok(PathsInfo {
        config_file: paths.config_file().display().to_string(),
        state_file: paths.state_file().display().to_string(),
        cache_dir: paths.cache_dir.display().to_string(),
        log_dir: paths.log_dir.display().to_string(),
    })
}

#[tauri::command]
pub async fn cli_status(state: State<'_, AppState>) -> CmdResult<CliStatus> {
    cli_link::status(&state.hw).await.cmd()
}

#[tauri::command]
pub async fn install_cli(state: State<'_, AppState>) -> CmdResult<CliStatus> {
    cli_link::install(&state.hw).await.cmd()
}

#[tauri::command]
pub async fn get_autostart(state: State<'_, AppState>) -> CmdResult<bool> {
    state.host()?.autostart_enabled().map_err(invalid)
}

#[tauri::command]
pub async fn set_autostart(state: State<'_, AppState>, enabled: bool) -> CmdResult<bool> {
    state.host()?.set_autostart(enabled).map_err(invalid)
}
