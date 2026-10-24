//! Bounded Headway bundle discovery and ownership-safe CLI links.
use crate::{
    Error, Headway, Result,
    paths::expand_tilde,
    process::{self, Cmd},
    shell::ShellEnv,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[expect(
    clippy::struct_excessive_bools,
    reason = "public CLI status wire contract"
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliStatus {
    pub bundled_path: Option<String>,
    pub link_path: String,
    pub installed: bool,
    pub points_to: Option<String>,
    pub on_path: bool,
    pub needs_repair: bool,
    pub resolved_path: Option<String>,
    pub shadowed: bool,
}

pub(crate) async fn bundle(hw: &Headway, explicit: Option<&str>) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit {
        let path = expand_tilde(path);
        validate_bundle(&path).await?;
        return Ok(Some(path));
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(app) = exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|s| s == "app"))
    {
        candidates.push(app.to_path_buf());
    }
    candidates.push(expand_tilde(&hw.settings()?.apps_dir).join("Headway.app"));
    for path in candidates {
        if path.exists() {
            validate_bundle(&path).await?;
            return Ok(Some(path));
        }
    }
    Ok(None)
}

async fn validate_bundle(path: &Path) -> Result<()> {
    let env = ShellEnv::current();
    let plist = path.join("Contents/Info.plist");
    for (key, expected) in [
        ("CFBundleIdentifier", "wang.frankie.headway"),
        ("CFBundleExecutable", "headway-desktop"),
    ] {
        let out = process::capture(&Cmd::new("/usr/libexec/PlistBuddy", &env).args([
            "-c".to_owned(),
            format!("Print :{key}"),
            plist.display().to_string(),
        ]))
        .await?;
        if !out.success() || out.stdout.trim() != expected {
            return Err(Error::InvalidInput(
                "not a validated Headway app bundle".into(),
            ));
        }
    }
    for name in ["headway", "headway-desktop"] {
        if env
            .which(&path.join("Contents/MacOS").join(name).display().to_string())
            .is_none()
        {
            return Err(Error::InvalidInput(
                "Headway bundle executable is missing".into(),
            ));
        }
    }
    Ok(())
}

fn owned(target: &Path) -> bool {
    target.ends_with("Headway.app/Contents/MacOS/headway")
        && !target
            .components()
            .any(|part| matches!(part.as_os_str().to_str(), Some("Cellar" | "Caskroom")))
}

pub(crate) fn repair(link: &Path, bundled: &Path) -> Result<()> {
    match std::fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target =
                std::fs::read_link(link).map_err(|e| Error::io_at("read link", link, e))?;
            if !owned(&target) {
                return Err(Error::InvalidInput(
                    "refusing to replace a link not owned by Headway".into(),
                ));
            }
        }
        Ok(_) => {
            return Err(Error::InvalidInput(
                "refusing to replace an existing file".into(),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io_at("inspect", link, e)),
    }
    let parent = link
        .parent()
        .ok_or_else(|| Error::InvalidInput("link has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(|e| Error::io_at("create", parent, e))?;
    let dir = tempfile::tempdir_in(parent).map_err(|e| Error::io_at("stage link", parent, e))?;
    let staged = dir.path().join("headway");
    std::os::unix::fs::symlink(bundled, &staged).map_err(|e| Error::io_at("link", &staged, e))?;
    std::fs::rename(&staged, link).map_err(|e| Error::io_at("replace link", link, e))
}

impl Headway {
    pub async fn cli_status(&self, app_path: Option<&str>) -> Result<CliStatus> {
        let bundled = bundle(self, app_path)
            .await?
            .map(|app| app.join("Contents/MacOS/headway"));
        let dir = expand_tilde(&self.settings()?.bin_dir);
        let link = dir.join("headway");
        let env = self.env().await;
        let resolved = env.which("headway");
        let installed = bundled.as_ref().is_some_and(|path| {
            std::fs::canonicalize(&link)
                .ok()
                .zip(std::fs::canonicalize(path).ok())
                .is_some_and(|(a, b)| a == b)
        });
        let shadowed = resolved.as_ref().is_some_and(|path| {
            std::fs::canonicalize(path).ok()
                != bundled.as_ref().and_then(|p| std::fs::canonicalize(p).ok())
        });
        Ok(CliStatus {
            bundled_path: bundled.map(|p| p.display().to_string()),
            link_path: link.display().to_string(),
            installed,
            points_to: std::fs::read_link(&link)
                .ok()
                .map(|p| p.display().to_string()),
            on_path: env.contains_dir(&dir),
            needs_repair: !installed,
            resolved_path: resolved.map(|p| p.display().to_string()),
            shadowed,
        })
    }
    pub async fn fix_cli_link(&self, app_path: Option<&str>) -> Result<CliStatus> {
        let app = bundle(self, app_path).await?.ok_or_else(|| {
            Error::InvalidInput("no installed Headway app found; provide --app".into())
        })?;
        let link = expand_tilde(&self.settings()?.bin_dir).join("headway");
        let _lock = crate::lock::AppLock::acquire(
            &self.paths().config_dir.join("cli-link.lock"),
            "cli-link",
        )?;
        repair(&link, &app.join("Contents/MacOS/headway"))?;
        self.cli_status(app_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repairs_dangling_headway_link() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("headway");
        std::os::unix::fs::symlink("/old/Headway.app/Contents/MacOS/headway", &link).unwrap();
        repair(&link, Path::new("/new/Headway.app/Contents/MacOS/headway")).unwrap();
        assert_eq!(
            std::fs::read_link(link).unwrap(),
            Path::new("/new/Headway.app/Contents/MacOS/headway")
        );
    }
    #[test]
    fn refuses_foreign_links_and_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("headway");
        for target in ["../Cellar/headway/0.1/bin/headway", "/other/tool"] {
            std::os::unix::fs::symlink(target, &link).unwrap();
            assert!(repair(&link, Path::new("/new/tool")).is_err());
            assert_eq!(std::fs::read_link(&link).unwrap(), Path::new(target));
            std::fs::remove_file(&link).unwrap();
        }
        std::fs::write(&link, "keep").unwrap();
        assert!(repair(&link, Path::new("/new/tool")).is_err());
        assert_eq!(std::fs::read_to_string(link).unwrap(), "keep");
    }
}

#[cfg(test)]
#[path = "cli_link_tests.rs"]
mod api_tests;
