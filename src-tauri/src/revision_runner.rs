use crate::state::{AppState, CmdResult, UPDATE_FINISHED, admit_run, apps_changed};
use headway_core::{ErrorInfo, RevisionRequest, VariantRequest};
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};

pub(crate) enum RevisionTask {
    InPlace {
        id: String,
        request: RevisionRequest,
    },
    Independent {
        parent_id: String,
        request: VariantRequest,
    },
}
impl RevisionTask {
    fn run_id(&self) -> &str {
        match self {
            Self::InPlace { id, .. } => id,
            Self::Independent { request, .. } => &request.id,
        }
    }
}
struct RunGuard<'a> {
    state: &'a AppState,
    id: &'a str,
}
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.state.end_run(self.id);
    }
}

pub(crate) fn spawn<R: Runtime>(app: AppHandle<R>, task: RevisionTask) -> CmdResult<()> {
    let id = task.run_id().to_owned();
    // Same admission as `runner::spawn_update`: the command learns immediately
    // that the start was refused.
    let cancel = admit_run(&app.state::<AppState>(), &id)?;
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _guard = RunGuard {
            state: &state,
            id: &id,
        };
        apps_changed(&app);
        let sink = crate::runner::WindowSink { app: app.clone() };
        let result = match task {
            RevisionTask::InPlace { id, request } => {
                state
                    .hw
                    .install_revision(&id, &request, &sink, &cancel)
                    .await
            }
            RevisionTask::Independent { parent_id, request } => {
                state
                    .hw
                    .install_variant(&parent_id, &request, &sink, &cancel)
                    .await
            }
        };
        let payload = crate::runner::UpdateFinished {
            app_id: id.clone(),
            ok: result.is_ok(),
            outcome: result.as_ref().ok().cloned(),
            error: result.as_ref().err().map(ErrorInfo::from),
        };
        let _ = app.emit(UPDATE_FINISHED, &payload);
        apps_changed(&app);
    });
    Ok(())
}
