//! Notices when `config.toml` / `state.json` change underneath the app (the CLI,
//! an agent, or a hand edit) and tells the window to refetch.

use std::time::Duration;

use tauri::{AppHandle, Manager as _, Runtime};

use crate::state::{AppState, apps_changed};

const POLL: Duration = Duration::from_secs(2);

pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut seen = app.state::<AppState>().hw.store().fingerprint();
        loop {
            tokio::time::sleep(POLL).await;
            let now = app.state::<AppState>().hw.store().fingerprint();
            if now != seen {
                seen = now;
                apps_changed(&app);
            }
        }
    });
}
