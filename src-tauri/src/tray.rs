//! Menu-bar icon: update count in the title, apps that are behind, quick actions.
//!
//! Labels come from [`crate::locale`] because the renderer's i18next cannot
//! translate a native menu. The language follows `Settings::language`.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};

use crate::locale::{self, Table};
use crate::runner;
use crate::state::{self, AppState, NAVIGATE, NavigatePayload, show_main};

pub const TRAY_ID: &str = "headway";
const MAX_APP_ITEMS: usize = 8;

/// The strings for the current persisted language.
fn table<R: Runtime>(app: &AppHandle<R>) -> &'static Table {
    locale::for_headway(&app.state::<AppState>().hw).strings()
}

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip(table(app).tooltip)
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

/// Rebuilds the menu and the count next to the icon.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = build_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
    let behind = behind_count(app);
    let title = (behind > 0).then(|| behind.to_string());
    let _ = tray.set_title(title.as_deref());
}

fn behind_count<R: Runtime>(app: &AppHandle<R>) -> usize {
    app.state::<AppState>().hw.list().map_or(0, |views| {
        views.iter().filter(|view| view.has_update()).count()
    })
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let views = app.state::<AppState>().hw.list().unwrap_or_default();
    let behind: Vec<_> = views.iter().filter(|view| view.has_update()).collect();
    let table = table(app);
    let menu = Menu::new(app)?;

    let header = match behind.len() {
        0 if views.is_empty() => table.no_apps.to_owned(),
        0 => table.up_to_date.to_owned(),
        count => table.updates_available(count),
    };
    menu.append(&MenuItem::with_id(
        app,
        "header",
        header,
        false,
        None::<&str>,
    )?)?;
    for view in behind.iter().take(MAX_APP_ITEMS) {
        let unit = match &view.app.source {
            headway_core::Source::Github(github) => match github.track {
                headway_core::Track::Commit { .. } => table.unit_commits,
                headway_core::Track::Release { .. } => table.unit_releases,
                headway_core::Track::Tag { .. } => table.unit_tags,
            },
            headway_core::Source::Npm(_)
            | headway_core::Source::Brew(_)
            | headway_core::Source::Script(_) => table.unit_versions,
        };
        let label = if view.busy {
            table.updating(&view.app.name)
        } else {
            table.behind(&view.app.name, view.pending_count, unit)
        };
        menu.append(&MenuItem::with_id(
            app,
            format!("app:{}", view.app.id),
            label,
            true,
            None::<&str>,
        )?)?;
    }
    if behind.len() > MAX_APP_ITEMS {
        menu.append(&MenuItem::with_id(
            app,
            "more",
            table.more(behind.len() - MAX_APP_ITEMS),
            false,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "check_now",
        table.check_now,
        true,
        None::<&str>,
    )?)?;
    if !behind.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "update_all",
            table.update_all,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "show",
        table.open,
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        table.quit,
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;
    Ok(menu)
}

fn handle_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "check_now" => app.state::<AppState>().wake_check(),
        "update_all" => runner::spawn_update_all(app),
        "show" => show_main(app),
        // Never `app.exit(0)` directly: an update may still be running, and the
        // drain in `state` waits for the cancellation to finish first.
        "quit" => state::request_quit(app),
        other => {
            if let Some(app_id) = other.strip_prefix("app:") {
                show_main(app);
                let _ = app.emit(
                    NAVIGATE,
                    NavigatePayload {
                        app_id: app_id.to_owned(),
                    },
                );
            }
        }
    }
}
