//! Headway desktop app: a menu-bar + window UI over `headway-core`, sharing its
//! config, state, logs and locks with the `headway` CLI.

mod cli_link;
mod commands;
#[cfg(test)]
mod ipc_tests;
mod runner;
mod scheduler;
mod state;
mod tray;
mod watcher;

use headway_core::{EnvMode, Headway};
use tauri::{Manager as _, RunEvent, Runtime, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use crate::state::{AppState, set_dock_visible, show_main};

/// Every command the window can call (shared with the IPC contract tests).
fn invoke_handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commands::apps::list_apps,
        commands::apps::get_app,
        commands::apps::app_history,
        commands::apps::add_app,
        commands::apps::save_app,
        commands::apps::remove_app,
        commands::apps::inspect_repo,
        commands::apps::lookup_source,
        commands::apps::list_presets,
        commands::ops::check_app,
        commands::ops::check_all,
        commands::ops::plan_update,
        commands::ops::start_update,
        commands::ops::cancel_update,
        commands::ops::mark_installed,
        commands::ops::read_log,
        commands::settings::get_settings,
        commands::settings::save_settings,
        commands::settings::token_status,
        commands::settings::set_token,
        commands::settings::clear_token,
        commands::settings::doctor,
        commands::settings::get_paths,
        commands::settings::cli_status,
        commands::settings::install_cli,
        commands::settings::get_autostart,
        commands::settings::set_autostart,
        commands::system::open_url,
        commands::system::reveal_path,
        commands::system::app_version,
        commands::system::get_build_info,
    ]
}

/// How long quitting waits for cancelled updates to stop.
const EXIT_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

pub fn run() {
    // Started from Finder the PATH is minimal: builds get the login shell's environment.
    let hw = match Headway::builder().env_mode(EnvMode::LoginShell).build() {
        Ok(hw) => hw,
        Err(err) => {
            eprintln!("headway: cannot start: {err}");
            std::process::exit(1);
        }
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::new(hw))
        .setup(|app| {
            let handle = app.handle();
            tray::create(handle)?;
            scheduler::start(handle.clone());
            watcher::start(handle.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Headway in the menu bar, checking in the background.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                set_dock_visible(window.app_handle(), false);
            }
        })
        .invoke_handler(invoke_handler())
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|handle, event| match event {
            // Quitting mid-update: stop the builds first, so none is left running unlocked.
            RunEvent::ExitRequested { api, .. } => {
                let state = handle.state::<AppState>();
                if !state.begin_exit() && state.cancel_all() > 0 {
                    api.prevent_exit();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let deadline = std::time::Instant::now() + EXIT_GRACE;
                        while handle.state::<AppState>().active_runs() > 0
                            && std::time::Instant::now() < deadline
                        {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        handle.exit(0);
                    });
                }
            }
            // Clicking the Dock icon brings the window back.
            RunEvent::Reopen { .. } => show_main(handle),
            _ => {}
        }),
        Err(err) => {
            eprintln!("headway: cannot start the app: {err}");
            std::process::exit(1);
        }
    }
}
