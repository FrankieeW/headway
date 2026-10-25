//! Host services the window may reach: opening links, revealing files in Finder,
//! and the login-item setting. They sit behind a trait so the IPC tests install a
//! recording double instead of touching the real browser, Finder, or login items.

use std::path::Path;

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

/// One host operation; small enough to implement per platform.
pub trait Host: Send + Sync {
    fn open_url(&self, url: &str) -> Result<(), String>;
    fn reveal(&self, path: &Path) -> Result<(), String>;
    fn autostart_enabled(&self) -> Result<bool, String>;
    /// Turns launch-at-login on/off and returns the resulting state.
    fn set_autostart(&self, enabled: bool) -> Result<bool, String>;
}

/// The real implementation: Tauri's opener and autostart plugins.
pub struct TauriHost {
    app: AppHandle<tauri::Wry>,
}

impl TauriHost {
    pub const fn new(app: AppHandle<tauri::Wry>) -> Self {
        Self { app }
    }
}

impl Host for TauriHost {
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|err| format!("open the link: {err}"))
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.app
            .opener()
            .reveal_item_in_dir(path)
            .map_err(|err| format!("reveal in Finder: {err}"))
    }

    fn autostart_enabled(&self) -> Result<bool, String> {
        self.app
            .autolaunch()
            .is_enabled()
            .map_err(|err| format!("read launch-at-login: {err}"))
    }

    fn set_autostart(&self, enabled: bool) -> Result<bool, String> {
        let launcher = self.app.autolaunch();
        let changed = if enabled {
            launcher.enable()
        } else {
            launcher.disable()
        };
        changed.map_err(|err| format!("change launch-at-login: {err}"))?;
        launcher
            .is_enabled()
            .map_err(|err| format!("read launch-at-login: {err}"))
    }
}
