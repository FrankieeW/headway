//! Running updates in the background and reporting them to the window.

use headway_core::{ErrorInfo, EventSink, Target, UpdateEvent, UpdateOutcome};
use serde::Serialize;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};
use tauri_plugin_notification::NotificationExt as _;

use crate::state::{AppState, UPDATE_EVENT, UPDATE_FINISHED, apps_changed};

pub(crate) struct WindowSink<R: Runtime> {
    pub(crate) app: AppHandle<R>,
}

impl<R: Runtime> EventSink for WindowSink<R> {
    fn emit(&self, event: UpdateEvent) {
        let _ = self.app.emit(UPDATE_EVENT, &event);
    }
}

#[derive(Debug, Clone, Serialize)]
struct UpdateFinished {
    app_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<UpdateOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorInfo>,
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

/// Runs one update to completion. Returns `false` if it was already running here.
pub async fn run_update<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    target: &Target,
    origin: Origin,
) -> bool {
    let state = app.state::<AppState>();
    let Some(cancel) = state.begin_run(id) else {
        return false;
    };
    // Ends the run even if the update panics, or this app could never update again.
    let _run = RunGuard { state: &state, id };
    apps_changed(app);
    let sink = WindowSink { app: app.clone() };
    let result = match origin {
        Origin::User => state.hw.update(id, target, &sink, &cancel).await.map(Some),
        Origin::Background => state
            .hw
            .auto_update(id, &sink, &cancel)
            .await
            .map(|runs| runs.last().cloned()),
    };

    let payload = match &result {
        Ok(outcome) => UpdateFinished {
            app_id: id.to_owned(),
            ok: true,
            outcome: outcome.clone(),
            error: None,
        },
        Err(err) => UpdateFinished {
            app_id: id.to_owned(),
            ok: false,
            outcome: None,
            error: Some(ErrorInfo::from(err)),
        },
    };
    let _ = app.emit(UPDATE_FINISHED, &payload);
    apps_changed(app);

    if origin == Origin::Background {
        let notify = state.hw.settings().is_ok_and(|settings| settings.notify);
        let body = match &result {
            Ok(Some(outcome)) if outcome.changed => Some(format!(
                "Updated {id} to {}",
                outcome.to.version.as_deref().unwrap_or(&outcome.to.id)
            )),
            Ok(_) => None,
            Err(err) => Some(format!("Updating {id} failed: {err}")),
        };
        if let (true, Some(body)) = (notify, body) {
            let _ = app
                .notification()
                .builder()
                .title("Headway")
                .body(body)
                .show();
        }
    }
    true
}

/// Starts an update without waiting for it.
pub fn spawn_update<R: Runtime>(app: &AppHandle<R>, id: String, target: Target, origin: Origin) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_update(&app, &id, &target, origin).await;
    });
}

/// Updates every app that is behind, one after another.
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
        for id in ids {
            run_update(&app, &id, &Target::Latest, Origin::User).await;
        }
    });
}
