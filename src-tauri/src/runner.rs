//! Running updates in the background and reporting them to the window.

use headway_core::{
    ErrorInfo, EventSink, RevisionMethod, Target, UpdateEvent, UpdateOutcome, UpdateRequest,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};
use tauri_plugin_notification::NotificationExt as _;

use crate::locale;
use crate::state::{AppState, CmdResult, UPDATE_EVENT, UPDATE_FINISHED, admit_run, apps_changed};

pub(crate) struct WindowSink<R: Runtime> {
    pub(crate) app: AppHandle<R>,
}

impl<R: Runtime> EventSink for WindowSink<R> {
    fn emit(&self, event: UpdateEvent) {
        let _ = self.app.emit(UPDATE_EVENT, &event);
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UpdateFinished {
    pub(crate) app_id: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) outcome: Option<UpdateOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<ErrorInfo>,
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

/// Who asked for the update; background runs notify when they finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    User,
    Background,
}

/// The result of one run in the shape the window and the notification need.
struct Finished {
    ok: bool,
    /// The last successful outcome; background runs report the steps that did
    /// land even when a later step failed.
    outcome: Option<UpdateOutcome>,
    error: Option<ErrorInfo>,
}

/// Runs one update to completion. Returns `false` when the run was not admitted
/// (already updating here, or the app is quitting/restarting).
pub async fn run_update<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    target: &Target,
    method: RevisionMethod,
    origin: Origin,
) -> bool {
    let state = app.state::<AppState>();
    let Ok(cancel) = admit_run(&state, id) else {
        return false;
    };
    run_admitted(app, id, target, method, origin, cancel).await;
    true
}

/// Starts an update without waiting for it.
///
/// Admission happens here, synchronously, so the command that asked can report a
/// refused start (the `busy` error) instead of leaving the window with a run that
/// never produces events.
pub fn spawn_update<R: Runtime>(
    app: &AppHandle<R>,
    id: String,
    target: Target,
    method: RevisionMethod,
    origin: Origin,
) -> CmdResult<()> {
    let cancel = admit_run(&app.state::<AppState>(), &id)?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_admitted(&app, &id, &target, method, origin, cancel).await;
    });
    Ok(())
}

/// Updates every app that is behind, one after another (tray "Update all").
pub fn spawn_update_all<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let ids: Vec<String> = app
            .state::<AppState>()
            .hw
            .list()
            .map(|views| {
                views
                    .into_iter()
                    .filter(|view| view.has_update() && !view.busy)
                    .map(|view| view.app.id)
                    .collect()
            })
            .unwrap_or_default();
        run_updates(&app, ids, Origin::User).await;
    });
}

/// Starts a background batch in its own task. The scheduler keeps its check loop
/// responsive while builds run: "Check now" and the next interval are not queued
/// behind a long build.
pub fn spawn_auto_updates<R: Runtime>(app: &AppHandle<R>, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_updates(&app, ids, Origin::Background).await;
    });
}

/// Runs the given updates sequentially, admitting each one separately so an app
/// that became busy (CLI, restart) is skipped instead of racing.
///
/// Production spawns this from the scheduler/tray; the scheduler tests await it
/// directly so their assertions are deterministic.
pub(crate) async fn run_updates<R: Runtime>(app: &AppHandle<R>, ids: Vec<String>, origin: Origin) {
    for id in ids {
        run_update(
            app,
            &id,
            &Target::Latest,
            RevisionMethod::Configured,
            origin,
        )
        .await;
    }
}

/// Runs one already-admitted update to completion.
async fn run_admitted<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    target: &Target,
    method: RevisionMethod,
    origin: Origin,
    cancel: headway_core::CancellationToken,
) {
    let state = app.state::<AppState>();
    // Ends the run even if the update panics, or this app could never update again.
    let _run = RunGuard { state: &state, id };
    apps_changed(app);
    let sink = WindowSink { app: app.clone() };
    let finished = match origin {
        Origin::User => match state
            .hw
            .update_with(
                id,
                &UpdateRequest {
                    target: target.clone(),
                    method,
                },
                &sink,
                &cancel,
            )
            .await
        {
            Ok(outcome) => Finished {
                ok: true,
                outcome: Some(outcome),
                error: None,
            },
            Err(error) => Finished {
                ok: false,
                outcome: None,
                error: Some(ErrorInfo::from(&error)),
            },
        },
        Origin::Background => {
            let report = state.hw.auto_update_report(id, &sink, &cancel).await;
            let outcome = report.outcomes.last().cloned();
            match report.error {
                None => Finished {
                    ok: true,
                    outcome,
                    error: None,
                },
                Some(error) => Finished {
                    ok: false,
                    outcome,
                    error: Some(error),
                },
            }
        }
    };

    let payload = UpdateFinished {
        app_id: id.to_owned(),
        ok: finished.ok,
        outcome: finished.outcome.clone(),
        error: finished.error.clone(),
    };
    let _ = app.emit(UPDATE_FINISHED, &payload);
    apps_changed(app);

    if origin == Origin::Background {
        notify(app, &state, id, &finished);
    }
}

/// Sends one native notification for a finished background run, if enabled.
fn notify<R: Runtime>(app: &AppHandle<R>, state: &AppState, id: &str, finished: &Finished) {
    if !state.hw.settings().is_ok_and(|settings| settings.notify) {
        return;
    }
    let table = locale::for_headway(&state.hw).strings();
    let name = state
        .hw
        .get(id)
        .map_or_else(|_| id.to_owned(), |view| view.app.name);
    let body = match (&finished.error, &finished.outcome) {
        (Some(error), _) => Some(table.update_failed(&name, &error.message)),
        (None, Some(outcome)) if outcome.changed => {
            let version = outcome.to.version.as_deref().unwrap_or(&outcome.to.id);
            Some(table.updated_to(&name, version))
        }
        (None, _) => None,
    };
    if let Some(body) = body {
        let _ = app
            .notification()
            .builder()
            .title(table.tooltip)
            .body(body)
            .show();
    }
}
