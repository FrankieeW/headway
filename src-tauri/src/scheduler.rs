//! Background checks every `check_interval_minutes`, with notifications for new
//! versions and auto-updates for apps that opted in.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use headway_core::Target;
use tauri::{AppHandle, Manager as _, Runtime};
use tauri_plugin_notification::NotificationExt as _;

use crate::runner::{self, Origin};
use crate::state::{AppState, apps_changed};

/// Let the app finish launching before the first check.
const STARTUP_DELAY: Duration = Duration::from_secs(15);
const IDLE_WAIT: Duration = Duration::from_secs(3600);

pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        let mut last_run: Option<Instant> = None;
        loop {
            let state = app.state::<AppState>();
            let minutes = state
                .hw
                .settings()
                .map_or(60, |settings| settings.check_interval_minutes);
            let interval = Duration::from_secs(u64::from(minutes) * 60);
            let forced = state.take_check_request();
            let due = minutes > 0 && last_run.is_none_or(|at| at.elapsed() >= interval);
            if due || forced {
                run_cycle(&app).await;
                last_run = Some(Instant::now());
            }
            let wait = if minutes == 0 {
                IDLE_WAIT
            } else {
                interval.saturating_sub(last_run.map_or(Duration::ZERO, |at| at.elapsed()))
            };
            let wake = std::sync::Arc::clone(&state.wake);
            tokio::select! {
                () = tokio::time::sleep(wait.max(Duration::from_secs(1))) => {}
                () = wake.notified() => {}
            }
        }
    });
}

async fn run_cycle<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let latest_before: HashMap<String, Option<String>> = state
        .hw
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|view| {
            let latest = view
                .last_check
                .and_then(|check| check.latest)
                .map(|latest| latest.id);
            (view.app.id, latest)
        })
        .collect();
    let Ok(results) = state.hw.check_many(&[]).await else {
        return;
    };
    apps_changed(app);

    let mut fresh = Vec::new();
    let mut auto = Vec::new();
    for (id, result) in results {
        let Ok(view) = result else { continue };
        if !view.has_update() {
            continue;
        }
        let latest = view
            .last_check
            .as_ref()
            .and_then(|check| check.latest.as_ref())
            .map(|latest| latest.id.clone());
        if latest_before
            .get(&id)
            .is_none_or(|before| *before != latest)
        {
            fresh.push(view.app.name.clone());
        }
        if view.app.auto_update && !view.busy {
            auto.push(id);
        }
    }

    let notify = state.hw.settings().is_ok_and(|settings| settings.notify);
    if notify && !fresh.is_empty() {
        let title = if fresh.len() == 1 {
            format!("{} has an update", fresh[0])
        } else {
            format!("{} apps have updates", fresh.len())
        };
        let _ = app
            .notification()
            .builder()
            .title(title)
            .body(fresh.join(", "))
            .show();
    }
    for id in auto {
        runner::run_update(app, &id, &Target::Latest, Origin::Background).await;
    }
}
