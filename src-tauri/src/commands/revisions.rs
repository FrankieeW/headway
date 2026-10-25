use crate::revision_runner::RevisionTask;
use crate::state::{AppState, CmdResult, IntoCmd as _, apps_changed};
use headway_core::{
    AppView, CommitPage, LaunchOutcome, RepositoryOption, RevisionOptions, RevisionRequest,
    SourceStorageStatus, UpdatePlan, VariantRequest,
};
use tauri::{AppHandle, Runtime, State};

#[tauri::command]
pub async fn detect_installation(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<headway_core::InstallationDetection> {
    state.hw.detect_installation(&id).cmd()
}

#[tauri::command]
pub async fn list_pull_requests(
    state: State<'_, AppState>,
    id: String,
    page: u32,
    per_page: u32,
) -> CmdResult<headway_core::PullRequestPage> {
    state.hw.list_pull_requests(&id, page, per_page).await.cmd()
}

#[tauri::command]
pub async fn refresh_repositories<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<AppView> {
    let view = state.hw.refresh_repositories(&id).await.cmd()?;
    apps_changed(&app);
    Ok(view)
}

#[tauri::command]
pub async fn source_storage_status(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<SourceStorageStatus> {
    state.hw.source_storage_status(&id).cmd()
}
#[tauri::command]
pub async fn list_commits(
    state: State<'_, AppState>,
    id: String,
    page: u32,
    per_page: u32,
    anchor: Option<String>,
) -> CmdResult<CommitPage> {
    state
        .hw
        .list_commits(&id, page, per_page, anchor.as_deref())
        .await
        .cmd()
}
#[tauri::command]
pub async fn revision_options(
    state: State<'_, AppState>,
    id: String,
    reference: String,
) -> CmdResult<RevisionOptions> {
    state.hw.revision_options(&id, &reference).await.cmd()
}
#[tauri::command]
pub async fn plan_revision(
    state: State<'_, AppState>,
    id: String,
    request: RevisionRequest,
) -> CmdResult<UpdatePlan> {
    state.hw.plan_revision(&id, &request).await.cmd()
}
#[tauri::command]
pub async fn plan_variant(
    state: State<'_, AppState>,
    parent_id: String,
    request: VariantRequest,
) -> CmdResult<UpdatePlan> {
    state.hw.plan_variant(&parent_id, &request).await.cmd()
}
#[tauri::command]
pub async fn install_revision<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    request: RevisionRequest,
) -> CmdResult<()> {
    let view = state.hw.get(&id).cmd()?;
    if view.busy {
        return Err(headway_core::ErrorInfo::from(&headway_core::Error::Busy(
            id,
        )));
    }
    view.app.require_github("commit installation").cmd()?;
    crate::revision_runner::spawn(app, RevisionTask::InPlace { id, request })
}
#[tauri::command]
pub async fn install_variant<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    parent_id: String,
    request: VariantRequest,
) -> CmdResult<()> {
    let parent = state.hw.get(&parent_id).cmd()?;
    if parent.busy {
        return Err(headway_core::ErrorInfo::from(&headway_core::Error::Busy(
            parent_id,
        )));
    }
    parent.app.require_github("independent versions").cmd()?;
    headway_core::repo::validate_app_id(&request.id).cmd()?;
    match state.hw.get(&request.id) {
        Ok(_) => {
            return Err(headway_core::ErrorInfo::from(
                &headway_core::Error::AppExists(request.id),
            ));
        }
        Err(headway_core::Error::AppNotFound(_)) => {}
        Err(error) => return Err(headway_core::ErrorInfo::from(&error)),
    }
    crate::revision_runner::spawn(app, RevisionTask::Independent { parent_id, request })
}
#[tauri::command]
pub async fn launch_variant(state: State<'_, AppState>, id: String) -> CmdResult<LaunchOutcome> {
    state.hw.launch_variant(&id).await.cmd()
}
#[tauri::command]
pub async fn list_repositories(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<RepositoryOption>> {
    state.hw.repositories(&id).cmd()
}
#[tauri::command]
pub async fn select_repository<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    repo: String,
) -> CmdResult<AppView> {
    let view = state.hw.select_repository(&id, &repo).cmd()?;
    apps_changed(&app);
    Ok(view)
}
