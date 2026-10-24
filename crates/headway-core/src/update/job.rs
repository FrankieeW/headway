use super::{Installed, Job, RunLog, file_name, human_bytes, resolve_artifact, short};
use crate::archive::{self, ArchiveKind};
use crate::asset::{self, Arch};
use crate::config::{AppKind, AssetInstall, Install, SourceInstall};
use crate::error::{Error, Result};
use crate::events::{LogStream, Stage};
use crate::git::{self, Checkout};
use crate::github::Asset;
use crate::install;
use crate::paths::expand_tilde;
use crate::process::{self, Cmd};
use crate::source::Source;
use crate::state::{Candidate, InstalledRef};
use std::fs;
use std::path::Path;

impl Job<'_> {
    pub async fn run(&self, target: &Candidate) -> Result<Installed> {
        match &self.app.source {
            Source::Github(github) => match &github.install {
                Install::Source(source) => self.install_source(source, target).await,
                Install::Asset(spec) => self.install_asset(spec, target).await,
            },
            Source::Npm(source) => crate::npm::install(self, source, target).await,
            Source::Brew(source) => crate::brew::install(self, source).await,
            Source::Script(source) => crate::script::install(self, source).await,
        }
    }

    pub(crate) fn output(&self) -> impl FnMut(LogStream, &str) + Send + '_ {
        move |stream, line| self.log.line(stream, line)
    }

    async fn install_source(
        &self,
        source: &SourceInstall,
        target: &Candidate,
    ) -> Result<Installed> {
        if source.artifact.is_none() && source.verify_command.is_some() {
            return Err(Error::InvalidInput(
                "post-install verification requires a managed artifact".into(),
            ));
        }
        let url = self.app.require_github("source builds")?.clone_url()?;
        let git_env = git::git_env(self.env, &url, self.token);
        let location =
            crate::source_storage::Location::new(&source.storage, self.paths, &self.app.id)?;
        let workspace = crate::source_storage::SourceCheckout::prepare(
            &location,
            &crate::source_storage::CheckoutContext {
                url: &url,
                env: &git_env,
                cancel: self.cancel,
            },
        )
        .await?;
        let dir = workspace.path().to_path_buf();
        let checkout = Checkout {
            dir: &dir,
            env: &git_env,
            cancel: self.cancel,
        };

        self.log.stage(Stage::Fetch, format!("Fetching {url}"));
        checkout.sync(&url, &mut self.output()).await?;

        let reference = checkout_ref(target);
        self.log.stage(
            Stage::Checkout,
            format!("Checking out {}", short(&reference)),
        );
        let sha = checkout.checkout(&reference, &mut self.output()).await?;
        let version = checkout.describe().await;

        let workdir = source
            .workdir
            .as_deref()
            .map_or_else(|| dir.clone(), |sub| dir.join(sub));
        self.log
            .stage(Stage::Build, format!("Running `{}`", source.build));
        let build_env = self
            .env
            .clone()
            .with_var("HEADWAY", "1")
            .with_var("HEADWAY_APP_ID", &self.app.id)
            .with_var("HEADWAY_REF", &target.id)
            .with_var("HEADWAY_SHA", &sha)
            .with_var("HEADWAY_VERSION", version.as_deref().unwrap_or(&target.id))
            .with_var(
                "HEADWAY_BIN_DIR",
                expand_tilde(&self.settings.bin_dir).display().to_string(),
            )
            .with_var(
                "HEADWAY_APPS_DIR",
                expand_tilde(&self.settings.apps_dir).display().to_string(),
            );
        let build = Cmd::new("/bin/sh", &build_env)
            .args(["-c", &source.build])
            .cwd(&workdir);
        process::run(&build, self.cancel, &mut self.output()).await?;

        let path = if let Some(artifact) = &source.artifact {
            let output = resolve_artifact(&workdir, artifact)?;
            let name = file_name(&output)?;
            let dest =
                install::resolve_dest(source.dest.as_deref(), self.app.kind, &name, self.settings);
            let candidate = InstalledRef {
                id: target.id.clone(),
                sha: Some(sha.clone()),
                version: version.clone(),
                installed_at: chrono::Utc::now(),
                path: None,
            };
            self.place(&output, &candidate, &dest).await?;
            Some(dest)
        } else {
            self.log
                .info("No artifact configured: the build command installs the program itself.");
            None
        };
        Ok(Installed {
            sha: Some(sha),
            version,
            path,
        })
    }

    async fn install_asset(&self, spec: &AssetInstall, target: &Candidate) -> Result<Installed> {
        let repo = self.app.repo_id()?;
        self.log
            .stage(Stage::Resolve, format!("Looking up release {}", target.id));
        let release = self.release_for_target(target).await?;
        let chosen = asset::select(
            &release.assets,
            self.app.kind,
            spec.pattern.as_deref(),
            Arch::host(),
        )?;

        let downloads = self.paths.downloads_dir();
        fs::create_dir_all(&downloads).map_err(|err| Error::io_at("create", &downloads, err))?;
        let scratch = tempfile::Builder::new()
            .prefix(&format!("{}-", self.app.id))
            .tempdir_in(&downloads)
            .map_err(|err| Error::io_at("create a temp dir in", &downloads, err))?;
        let file = scratch.path().join(&chosen.name);

        self.log.stage(
            Stage::Download,
            format!("Downloading {} ({})", chosen.name, human_bytes(chosen.size)),
        );
        let mut progress = |done, total| self.log.progress(done, total);
        let digest = tokio::select! {
            digest = self.gh.download_asset(&repo, chosen, &file, &mut progress) => digest?,
            () = self.cancel.cancelled() => return Err(Error::Cancelled),
        };
        verify_digest(chosen, &digest, self.log)?;

        self.log
            .stage(Stage::Unpack, format!("Unpacking {}", chosen.name));
        let kind = archive::detect(&chosen.name);
        let unpacked = archive::unpack(
            &file,
            kind,
            &scratch.path().join("unpacked"),
            self.env,
            self.cancel,
            &mut self.output(),
        )
        .await?;
        let (payload, dest_name) = match self.app.kind {
            AppKind::Cli => {
                let wanted = spec.binary.clone().unwrap_or_else(|| repo.name.clone());
                let payload = archive::find_binary(&unpacked.root, &wanted)?;
                // A bare asset is named like `jq-macos-arm64`: install it as the tool's name.
                let name = if kind == ArchiveKind::Raw || spec.binary.is_some() {
                    wanted
                } else {
                    file_name(&payload)?
                };
                (payload, name)
            }
            AppKind::App => {
                let payload = archive::find_app_bundle(&unpacked.root, spec.binary.as_deref())?;
                let name = file_name(&payload)?;
                (payload, name)
            }
        };
        let dest = install::resolve_dest(
            spec.dest.as_deref(),
            self.app.kind,
            &dest_name,
            self.settings,
        );
        let candidate = InstalledRef {
            id: target.id.clone(),
            sha: target.sha.clone(),
            version: Some(target.id.clone()),
            installed_at: chrono::Utc::now(),
            path: None,
        };
        self.place(&payload, &candidate, &dest).await?;
        drop(unpacked);
        Ok(Installed {
            sha: None,
            version: Some(target.id.clone()),
            path: Some(dest),
        })
    }

    async fn place(&self, src: &Path, candidate: &InstalledRef, dest: &Path) -> Result<()> {
        crate::recovery::Replacement {
            app: self.app,
            paths: self.paths,
            env: self.env,
            log: self.log,
            cancel: self.cancel,
        }
        .place(src, Some(candidate), dest)
        .await
    }
}

fn verify_digest(chosen: &Asset, actual: &str, log: &RunLog<'_>) -> Result<()> {
    match chosen
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
    {
        Some(expected) if expected.eq_ignore_ascii_case(actual) => {
            log.stage(
                Stage::Verify,
                "SHA-256 matches the digest published by GitHub",
            );
            Ok(())
        }
        Some(expected) => Err(Error::Checksum {
            asset: chosen.name.clone(),
            expected: expected.to_owned(),
            actual: actual.to_owned(),
        }),
        None => {
            log.info(&format!(
                "GitHub published no digest for this asset (sha256 {actual})."
            ));
            Ok(())
        }
    }
}

/// The ref git checks out: the commit SHA, or the tag for releases without one.
fn checkout_ref(target: &Candidate) -> String {
    target
        .sha
        .clone()
        .unwrap_or_else(|| format!("refs/tags/{}", target.id))
}
