//! Opening links and folders.

use tauri::{AppHandle, Runtime};
use tauri_plugin_opener::OpenerExt as _;

use crate::state::{CmdResult, invalid};

/// Opens an http(s) link in the default browser; anything else is refused.
#[tauri::command]
pub async fn open_url<R: Runtime>(app: AppHandle<R>, url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(invalid(format!(
            "refusing to open `{url}`: only http(s) links are allowed"
        )));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| invalid(format!("open the link: {err}")))
}

#[tauri::command]
pub async fn reveal_path<R: Runtime>(app: AppHandle<R>, path: String) -> CmdResult<()> {
    let path = headway_core::paths::expand_tilde(&path);
    if !path.exists() {
        return Err(invalid(format!("{} does not exist", path.display())));
    }
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|err| invalid(format!("reveal in Finder: {err}")))
}

#[tauri::command]
pub async fn app_version<R: Runtime>(app: AppHandle<R>) -> String {
    app.package_info().version.to_string()
}
