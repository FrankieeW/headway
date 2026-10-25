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
        super::reconcile(self.paths, self.app)?;
        let cache = Cache::new(self.paths, self.app)?;
        install::validate(src, dest)?;
        let payload = src.to_owned();
        tokio::task::spawn_blocking(move || crate::archive::validate_tree(&payload))
            .await
            .map_err(|error| Error::io("payload budget worker", std::io::Error::other(error)))??;
        let previous = Store::new(self.paths.clone())
            .state()?
            .app(&self.app.id)
            .installed;
        let snapshot_previous = previous.clone().filter(|old| {
            old.path
                .as_deref()
                .is_none_or(|path| crate::paths::expand_tilde(path) == dest)
        });
        let mut intent = super::journal::Intent::create(
            self.paths,
            self.app,
            dest,
            candidate,
            previous.clone(),
        )?;
        let pending = if dest.exists() {
            let paths = self.paths.clone();
            let app = self.app.clone();
            let dest = dest.to_owned();
            Some(
                tokio::task::spawn_blocking(move || {
                    Cache::new(&paths, &app)?.snapshot(&dest, snapshot_previous)
                })
                .await
                .map_err(|error| Error::io("snapshot worker", std::io::Error::other(error)))??,
            )
        } else {
            None
        };
        self.log
            .stage(Stage::Install, format!("Installing {}", dest.display()));
        let src = src.to_owned();
        let destination = dest.to_owned();
        let mut transaction =
            tokio::task::spawn_blocking(move || install::Transaction::begin(&src, &destination))
                .await
                .map_err(|error| Error::io("install worker", std::io::Error::other(error)))??;
        if let Err(error) = self.verify(dest, candidate, command).await {
            if let Err(rollback_error) = transaction.rollback() {
                let rescue = pending.map(super::cache::Pending::preserve);
                Store::new(self.paths.clone()).update_state(|state| {
                    Ok((
                        state.with_app(&self.app.id, |state| crate::state::AppState {
                            installed: None,
                            last_check: None,
                            ..state
                        }),
                        (),
                    ))
                })?;
                return Err(Error::io(
                    format!(
                        "verification rollback failed; installed version is unknown; rescue payload {}",
                        rescue.as_deref().map_or_else(
                            || "retained in install journal".into(),
                            |path| path.display().to_string()
                        )
                    ),
                    std::io::Error::other(rollback_error),
                ));
            }
            super::journal::clear(self.paths, self.app)?;
            return Err(error);
        }
        intent.verified(self.paths, self.app)?;
        // The service owns the state commit. Until it commits, a restart rolls
        // this replacement back rather than attributing uncommitted bytes.
        transaction.defer();
        if let Some(pending) = pending {
            let retained = tokio::task::spawn_blocking(move || cache.retain(pending)).await;
            match retained {
                Ok(Ok(())) => {}
                other => self.log.info(&format!(
                    "Installed and verified, but backup retention failed: {other:?}"
                )),
            }
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
        let provenance = crate::build_info::BuildInfo::current();
        let known_self = self.app.github().is_some_and(|github| {
            provenance
                .repository
                .as_deref()
                .is_some_and(|repo| github.repo.eq_ignore_ascii_case(repo))
        });
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
                    .timeout(std::time::Duration::from_secs(5 * 60))
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
                &Cmd::new("/bin/sh", &env)
                    .args(["-c", command])
                    .timeout(std::time::Duration::from_secs(5 * 60)),
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
