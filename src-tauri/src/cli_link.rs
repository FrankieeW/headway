//! "Install the `headway` command": a symlink from `bin_dir` to the CLI shipped
//! next to the app binary (`Headway.app/Contents/MacOS/headway`, or
//! `target/<profile>/headway` during development).

use std::path::PathBuf;

use headway_core::paths::expand_tilde;
use headway_core::{Error, Headway, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CliStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    bundled_path: Option<String>,
    link_path: String,
    installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    points_to: Option<String>,
    on_path: bool,
}

fn bundled_cli() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join("headway");
    candidate.is_file().then_some(candidate)
}

pub async fn status(hw: &Headway) -> Result<CliStatus> {
    let bin_dir = expand_tilde(&hw.settings()?.bin_dir);
    let link = bin_dir.join("headway");
    let env = hw.env().await;
    Ok(CliStatus {
        bundled_path: bundled_cli().map(|path| path.display().to_string()),
        link_path: link.display().to_string(),
        installed: link.exists(),
        points_to: std::fs::read_link(&link)
            .ok()
            .map(|target| target.display().to_string()),
        on_path: env.contains_dir(&bin_dir),
    })
}

pub async fn install(hw: &Headway) -> Result<CliStatus> {
    let bundled = bundled_cli().ok_or_else(|| {
        Error::InvalidInput("this build has no bundled `headway` CLI next to the app binary".into())
    })?;
    let bin_dir = expand_tilde(&hw.settings()?.bin_dir);
    std::fs::create_dir_all(&bin_dir).map_err(|err| Error::io_at("create", &bin_dir, err))?;
    let link = bin_dir.join("headway");
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::remove_file(&link).map_err(|err| Error::io_at("replace", &link, err))?;
        }
        Ok(_) => {
            return Err(Error::InvalidInput(format!(
                "{} already exists and is not a link made by Headway; remove it first",
                link.display()
            )));
        }
        Err(_) => {}
    }
    std::os::unix::fs::symlink(&bundled, &link).map_err(|err| Error::io_at("link", &link, err))?;
    status(hw).await
}
