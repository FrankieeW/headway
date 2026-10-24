//! Persistent, one-shot desktop restart requests shared with the CLI.
use crate::{BuildInfo, Error, Headway, Result, process::Cmd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfUpdateStatus {
    pub running: BuildInfo,
    pub installed: Option<BuildInfo>,
    pub pending_restart: bool,
    pub installed_path: Option<String>,
    pub app_path: Option<String>,
    pub restart_requested: bool,
    pub confirmed: bool,
    #[serde(default)]
    pub restart_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RestartRequest {
    installed: BuildInfo,
    app_path: String,
    dispatched: bool,
    confirmed: bool,
    #[serde(default)]
    restart_error: Option<String>,
}

#[derive(Deserialize)]
struct VersionResult {
    ok: bool,
    data: BuildInfo,
}

fn same_build(a: &BuildInfo, b: &BuildInfo) -> bool {
    a.commit.is_some() && a == b && !a.dirty && !b.dirty
}

fn inside_bundle(exe: &std::path::Path, bundle: &str) -> bool {
    let root = std::path::PathBuf::from(bundle);
    exe.canonicalize()
        .unwrap_or_else(|_| exe.to_owned())
        .starts_with(root.canonicalize().unwrap_or(root))
}

impl Headway {
    fn restart_file(&self) -> std::path::PathBuf {
        self.paths().config_dir.join("self-restart.json")
    }
    fn restart_request(&self) -> Result<Option<RestartRequest>> {
        let path = self.restart_file();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| Error::parse(path.display().to_string(), &e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io_at("read restart request", &path, e)),
        }
    }
    fn save_restart(&self, request: &RestartRequest) -> Result<()> {
        let bytes = serde_json::to_vec(request).map_err(|e| Error::parse("restart request", &e))?;
        crate::store::write_atomic(&self.restart_file(), &bytes)
    }
    pub async fn self_update_status(&self) -> Result<SelfUpdateStatus> {
        let app = crate::cli_link::bundle(self, None).await?;
        let installed_path = app.as_ref().map(|p| p.join("Contents/MacOS/headway"));
        let installed = if let Some(path) = &installed_path {
            let output = crate::probe::capture(
                &crate::CancellationToken::new(),
                &Cmd::new(path.display().to_string(), &self.env().await)
                    .args(["--json", "version"]),
            )
            .await?;
            if !output.success() {
                return Err(Error::InvalidInput(
                    "installed CLI version probe failed".into(),
                ));
            }
            let version: VersionResult = serde_json::from_str(&output.stdout)
                .map_err(|e| Error::parse("installed CLI version", &e))?;
            if !version.ok {
                return Err(Error::InvalidInput(
                    "installed CLI rejected version probe".into(),
                ));
            }
            Some(version.data)
        } else {
            None
        };
        let running = BuildInfo::current();
        let request = self.restart_request()?;
        Ok(SelfUpdateStatus {
            pending_restart: installed
                .as_ref()
                .is_some_and(|info| !same_build(info, &running)),
            installed_path: installed_path.map(|p| p.display().to_string()),
            app_path: app.map(|p| p.display().to_string()),
            restart_requested: request
                .as_ref()
                .is_some_and(|r| !r.confirmed && r.restart_error.is_none()),
            restart_error: request.as_ref().and_then(|r| r.restart_error.clone()),
            confirmed: request.as_ref().is_some_and(|r| {
                r.confirmed
                    && installed
                        .as_ref()
                        .is_some_and(|i| same_build(i, &r.installed))
            }),
            running,
            installed,
        })
    }
    pub async fn request_self_restart(&self) -> Result<SelfUpdateStatus> {
        let status = self.self_update_status().await?;
        let app_path = status.app_path.ok_or_else(|| {
            Error::InvalidInput("no installed desktop app is available to restart".into())
        })?;
        let installed = status
            .installed
            .ok_or_else(|| Error::InvalidInput("installed build metadata unavailable".into()))?;
        if installed.commit.is_none() || installed.dirty {
            return Err(Error::InvalidInput(
                "restart confirmation requires a clean stamped installed build".into(),
            ));
        }
        let _lock = crate::lock::AppLock::acquire(
            &self.paths().config_dir.join("self-restart.lock"),
            "self-restart",
        )?;
        self.save_restart(&RestartRequest {
            installed,
            app_path,
            dispatched: false,
            confirmed: false,
            restart_error: None,
        })?;
        self.self_update_status().await
    }
    /// Called only by desktop startup, never by a CLI status probe.
    pub fn confirm_self_restart(&self) -> Result<bool> {
        let exe = std::env::current_exe().map_err(|e| Error::io("current executable", e))?;
        self.confirm_running_build(&BuildInfo::current(), &exe)
    }
    fn confirm_running_build(&self, running: &BuildInfo, exe: &std::path::Path) -> Result<bool> {
        let _lock = crate::lock::AppLock::acquire(
            &self.paths().config_dir.join("self-restart.lock"),
            "self-restart",
        )?;
        let Some(mut request) = self.restart_request()? else {
            return Ok(false);
        };
        if !request.dispatched || request.confirmed || request.restart_error.is_some() {
            return Ok(request.confirmed);
        }
        if !inside_bundle(exe, &request.app_path) {
            return Ok(false);
        }
        if same_build(&request.installed, running) {
            request.confirmed = true;
        } else {
            request.restart_error = Some("Restarted desktop does not match the requested installed build; retry after checking the installed app.".into());
        }
        self.save_restart(&request)?;
        Ok(request.confirmed)
    }
    /// Consumes the request before restarting, preventing a failed launch loop.
    pub fn take_self_restart(&self) -> Result<bool> {
        self.take_self_restart_when(|| true)
    }
    pub fn take_self_restart_when(&self, admit: impl FnOnce() -> bool) -> Result<bool> {
        let _lock = crate::lock::AppLock::acquire(
            &self.paths().config_dir.join("self-restart.lock"),
            "self-restart",
        )?;
        let Some(mut request) = self.restart_request()? else {
            return Ok(false);
        };
        if request.dispatched || request.confirmed || request.restart_error.is_some() {
            return Ok(false);
        }
        let exe = std::env::current_exe().map_err(|e| Error::io("current executable", e))?;
        if !inside_bundle(&exe, &request.app_path) {
            return Ok(false);
        }
        if !admit() {
            return Ok(false);
        }
        request.dispatched = true;
        self.save_restart(&request)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_requires_clean_matching_commit() {
        let build = BuildInfo {
            version: "1".into(),
            commit: Some("a".repeat(40)),
            dirty: false,
            target: None,
        };
        assert!(same_build(&build, &build));
        assert!(!same_build(
            &build,
            &BuildInfo {
                dirty: true,
                ..build.clone()
            }
        ));
        assert!(!same_build(
            &build,
            &BuildInfo {
                commit: None,
                ..build.clone()
            }
        ));
    }
    #[test]
    fn consumed_request_does_not_restart_again() {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(crate::paths::Paths::under(dir.path()))
            .build()
            .unwrap();
        hw.save_restart(&RestartRequest {
            installed: BuildInfo::current(),
            app_path: "/".into(),
            dispatched: false,
            confirmed: false,
            restart_error: None,
        })
        .unwrap();
        assert!(hw.take_self_restart().unwrap());
        assert!(!hw.take_self_restart().unwrap());
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn matching_desktop_start_confirms_consumed_request() {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(crate::paths::Paths::under(dir.path()))
            .build()
            .unwrap();
        let build = BuildInfo {
            version: "0.2.1".into(),
            commit: Some("a".repeat(40)),
            dirty: false,
            target: None,
        };
        hw.save_restart(&RestartRequest {
            installed: build.clone(),
            app_path: "/Headway.app".into(),
            dispatched: true,
            confirmed: false,
            restart_error: None,
        })
        .unwrap();
        assert!(
            hw.confirm_running_build(
                &build,
                std::path::Path::new("/Headway.app/Contents/MacOS/headway-desktop")
            )
            .unwrap()
        );
        let request = hw.restart_request().unwrap().unwrap();
        assert!(request.confirmed);
        assert!(request.restart_error.is_none());
        assert!(!hw.take_self_restart().unwrap());
    }
    #[test]
    fn busy_admission_preserves_request_for_later() {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(crate::paths::Paths::under(dir.path()))
            .build()
            .unwrap();
        hw.save_restart(&RestartRequest {
            installed: BuildInfo::current(),
            app_path: "/".into(),
            dispatched: false,
            confirmed: false,
            restart_error: None,
        })
        .unwrap();
        assert!(!hw.take_self_restart_when(|| false).unwrap());
        assert!(!hw.restart_request().unwrap().unwrap().dispatched);
        assert!(hw.take_self_restart_when(|| true).unwrap());
    }
    #[test]
    fn mismatched_running_build_is_not_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let hw = Headway::builder()
            .paths(crate::paths::Paths::under(dir.path()))
            .build()
            .unwrap();
        hw.save_restart(&RestartRequest {
            installed: BuildInfo {
                commit: Some("f".repeat(40)),
                ..BuildInfo::current()
            },
            app_path: "/".into(),
            dispatched: true,
            confirmed: false,
            restart_error: None,
        })
        .unwrap();
        assert!(!hw.confirm_self_restart().unwrap());
        let failed = hw.restart_request().unwrap().unwrap();
        assert!(!failed.confirmed);
        assert!(failed.restart_error.is_some());
        assert!(!hw.take_self_restart().unwrap());
    }
}
