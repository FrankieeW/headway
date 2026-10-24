use std::path::Path;

use crate::config::TrackedApp;
use crate::error::{Error, Result};
use crate::events::Stage;
use crate::install;
use crate::paths::Paths;
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;
use crate::state::InstalledRef;
use crate::store::Store;
use crate::update::RunLog;
use tokio_util::sync::CancellationToken;

use super::{Cache, verification};

pub(crate) struct Replacement<'a> {
    pub app: &'a TrackedApp,
    pub paths: &'a Paths,
    pub env: &'a ShellEnv,
    pub log: &'a RunLog<'a>,
    pub cancel: &'a CancellationToken,
}

impl Replacement<'_> {
    pub async fn place(
        &self,
        src: &Path,
        candidate: Option<&InstalledRef>,
        dest: &Path,
    ) -> Result<()> {
        let command = verification(self.app)?;
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let cache = Cache::new(self.paths, self.app)?;
        cache.list()?;
        install::validate(src, dest)?;
        let previous = Store::new(self.paths.clone())
            .state()?
            .app(&self.app.id)
            .installed
            .filter(|old| {
                old.path
                    .as_deref()
                    .is_some_and(|path| crate::paths::expand_tilde(path) == dest)
            });
        let pending = if dest.exists() {
            Some(cache.snapshot(dest, previous)?)
        } else {
            None
        };
        self.log
            .stage(Stage::Install, format!("Installing {}", dest.display()));
        let mut rollback = Rollback {
            dest,
            pending,
            armed: true,
            paths: self.paths,
            id: &self.app.id,
        };
        let result = async {
            install::install(src, dest)?;
            self.verify(dest, candidate, command).await
        }
        .await;
        if let Err(error) = result {
            rollback.restore()?;
            return Err(error);
        }
        rollback.armed = false;
        if let Some(pending) = rollback.pending.take()
            && let Err(error) = cache.retain(pending)
        {
            self.log.info(&format!(
                "Installed and verified, but backup retention failed: {error}"
            ));
        }
        Ok(())
    }

    async fn verify(
        &self,
        dest: &Path,
        candidate: Option<&InstalledRef>,
        command: Option<&str>,
    ) -> Result<()> {
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let known_self = self
            .app
            .github()
            .is_some_and(|github| github.repo.eq_ignore_ascii_case("FrankieeW/headway"));
        let env = self
            .env
            .clone()
            .with_var("HEADWAY_INSTALLED_PATH", dest.display().to_string())
            .with_var("HEADWAY_APP_ID", &self.app.id)
            .with_var(
                "HEADWAY_SHA",
                candidate.and_then(|c| c.sha.as_deref()).unwrap_or_default(),
            )
            .with_var(
                "HEADWAY_REF",
                candidate.map(|c| c.id.as_str()).unwrap_or_default(),
            )
            .with_var(
                "HEADWAY_VERSION",
                candidate
                    .and_then(|c| c.version.as_deref())
                    .unwrap_or_default(),
            );
        let mut output = |stream, line: &str| self.log.line(stream, line);
        if cfg!(target_os = "macos")
            && dest.is_dir()
            && dest
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            && (command.is_some() || known_self)
        {
            self.log
                .stage(Stage::Verify, "Verifying installed code signature");
            process::run(
                &Cmd::new("/usr/bin/codesign", &env)
                    .args(["--verify", "--deep", "--strict"])
                    .arg(dest.display().to_string()),
                self.cancel,
                &mut output,
            )
            .await?;
        }
        if let Some(command) = command {
            self.log.stage(
                Stage::Verify,
                "Running configured post-install verification",
            );
            process::run(
                &Cmd::new("/bin/sh", &env).args(["-c", command]),
                self.cancel,
                &mut output,
            )
            .await?;
        }
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

struct Rollback<'a> {
    dest: &'a Path,
    pending: Option<super::cache::Pending>,
    armed: bool,
    paths: &'a Paths,
    id: &'a str,
}

impl Rollback<'_> {
    fn restore(&mut self) -> Result<()> {
        self.armed = false;
        let result = match &self.pending {
            Some(old) => install::install(&old.payload, self.dest),
            None => install::remove_any(self.dest),
        };
        let rescue = if result.is_err() {
            self.pending.take().map(super::cache::Pending::preserve)
        } else {
            None
        };
        if result.is_err() || self.pending.is_none() {
            Store::new(self.paths.clone()).update_state(|state| {
                Ok((
                    state.with_app(self.id, |app| crate::state::AppState {
                        installed: None,
                        last_check: None,
                        ..app
                    }),
                    (),
                ))
            })?;
        }
        result.map_err(|error| {
            Error::io(
                format!("verification rollback failed; installed version is unknown; previous payload retained at {}",
                    rescue.as_deref().map_or_else(|| "none".into(), |path| path.display().to_string())),
                std::io::Error::other(error),
            )
        })
    }
}

impl Drop for Rollback<'_> {
    fn drop(&mut self) {
        if self.armed
            && let Err(error) = self.restore()
        {
            eprintln!("Headway rollback failed: {error}");
        }
    }
}
