//! Installing one target (a commit or a release) for an app: the build or
//! download pipeline itself. Recording the result is up to [`crate::service`].

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::archive::{self, ArchiveKind};
use crate::asset::{self, Arch};
use crate::config::{AppKind, AssetInstall, Install, Settings, SourceInstall, TrackedApp};
use crate::error::{Error, Result};
use crate::events::{EventSink, LogStream, Stage, UpdateEvent};
use crate::git::{self, Checkout};
use crate::github::{Asset, GitHub};
use crate::install;
use crate::paths::{Paths, display_path, expand_tilde};
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;
use crate::state::{Candidate, InstalledRef};

/// Log files kept per app.
pub const LOG_KEEP: usize = 20;

/// What to install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// The newest commit on the branch / the newest release or tag.
    Latest,
    /// The candidate right after the installed one (one commit or version at a time).
    Next,
    /// A specific commit SHA, branch or tag.
    Ref(String),
}

impl Target {
    /// `latest`, `next`, or anything else as a ref.
    pub fn parse(input: &str) -> Self {
        match input.trim() {
            "latest" | "" => Self::Latest,
            "next" => Self::Next,
            other => Self::Ref(other.to_owned()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Latest => "the latest version".to_owned(),
            Self::Next => "the next version".to_owned(),
            Self::Ref(reference) => format!("`{reference}`"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetChoice {
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// What `update --dry-run` prints.
#[derive(Debug, Clone, Serialize)]
pub struct UpdatePlan {
    pub app_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<InstalledRef>,
    pub to: Candidate,
    /// The target is what is installed already; a real run would do nothing.
    pub up_to_date: bool,
    /// `--to` the installed version: a real run builds/downloads it again.
    pub reinstall: bool,
    pub method: &'static str,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetChoice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateOutcome {
    pub app_id: String,
    /// `false` when the target was already installed.
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<InstalledRef>,
    pub to: InstalledRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    pub duration_ms: u64,
}

/// The log of one run: written to a file and mirrored to the event sink.
pub struct RunLog<'a> {
    app_id: String,
    path: PathBuf,
    file: Mutex<File>,
    sink: &'a dyn EventSink,
    last_progress: Mutex<Option<Instant>>,
}

impl<'a> RunLog<'a> {
    pub fn create(dir: &Path, app_id: &str, sink: &'a dyn EventSink) -> Result<Self> {
        fs::create_dir_all(dir).map_err(|err| Error::io_at("create", dir, err))?;
        let path = dir.join(format!("{}.log", Utc::now().format("%Y%m%d-%H%M%S%.3f")));
        let file = File::create(&path).map_err(|err| Error::io_at("create", &path, err))?;
        prune_logs(dir, LOG_KEEP);
        Ok(Self {
            app_id: app_id.to_owned(),
            path,
            file: Mutex::new(file),
            sink,
            last_progress: Mutex::new(None),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn line(&self, stream: LogStream, text: &str) {
        self.write(stream, text);
        self.sink.emit(UpdateEvent::Log {
            app_id: self.app_id.clone(),
            stream,
            line: text.to_owned(),
        });
    }

    pub fn info(&self, text: &str) {
        self.line(LogStream::Info, text);
    }

    /// Starts a stage. The message goes to the file and, once, to the sink as a stage event.
    pub fn stage(&self, stage: Stage, message: impl Into<String>) {
        let message = message.into();
        self.write(LogStream::Info, &message);
        self.sink.emit(UpdateEvent::Stage {
            app_id: self.app_id.clone(),
            stage,
            message,
        });
    }

    fn write(&self, stream: LogStream, text: &str) {
        if let Ok(mut file) = self.file.lock() {
            let tag = match stream {
                LogStream::Stdout => "out",
                LogStream::Stderr => "err",
                LogStream::Info => "headway",
            };
            let _ = writeln!(file, "{} [{tag}] {text}", Utc::now().format("%H:%M:%S"));
        }
    }

    /// Reports download progress, at most every 200 ms (and always at the end).
    pub fn progress(&self, done: u64, total: Option<u64>) {
        let finished = total.is_some_and(|total| done >= total);
        if let Ok(mut last) = self.last_progress.lock() {
            let due = last.is_none_or(|at| at.elapsed() >= Duration::from_millis(200));
            if !(due || finished) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.sink.emit(UpdateEvent::Progress {
            app_id: self.app_id.clone(),
            done,
            total,
        });
    }
}

/// Deletes all but the newest `keep` log files in `dir`.
fn prune_logs(dir: &Path, keep: usize) {
    let mut logs = log_files(dir);
    let excess = logs.len().saturating_sub(keep);
    logs.sort();
    for old in logs.into_iter().take(excess) {
        let _ = fs::remove_file(old);
    }
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
                .collect()
        })
        .unwrap_or_default()
}

/// The newest log file in `dir`.
pub fn latest_log(dir: &Path) -> Option<PathBuf> {
    log_files(dir).into_iter().max()
}

/// Result of a successful job.
#[derive(Debug, Clone)]
pub struct Installed {
    pub sha: Option<String>,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

/// Everything one update run needs.
pub struct Job<'a> {
    pub app: &'a TrackedApp,
    pub settings: &'a Settings,
    pub paths: &'a Paths,
    pub gh: &'a GitHub,
    pub env: &'a ShellEnv,
    /// Used for git over HTTPS; never handed to build commands.
    pub token: Option<&'a str>,
    pub cancel: &'a CancellationToken,
    pub log: &'a RunLog<'a>,
}

impl Job<'_> {
    pub async fn run(&self, target: &Candidate) -> Result<Installed> {
        match &self.app.install {
            Install::Source(source) => self.install_source(source, target).await,
            Install::Asset(spec) => self.install_asset(spec, target).await,
        }
    }

    fn output(&self) -> impl FnMut(LogStream, &str) + Send + '_ {
        move |stream, line| self.log.line(stream, line)
    }

    async fn install_source(
        &self,
        source: &SourceInstall,
        target: &Candidate,
    ) -> Result<Installed> {
        let url = self.app.clone_url()?;
        let dir = self.paths.source_dir(&self.app.id);
        let git_env = git::git_env(self.env, &url, self.token);
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
            self.place(&output, artifact, &dest).await?;
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
        let release = self.gh.release_by_tag(&repo, &target.id).await?;
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
        let label = payload.strip_prefix(&unpacked.root).map_or_else(
            |_| display_path(&payload),
            |inside| inside.display().to_string(),
        );
        self.place(&payload, &label, &dest).await?;
        drop(unpacked);
        Ok(Installed {
            sha: None,
            version: Some(target.id.clone()),
            path: Some(dest),
        })
    }

    async fn place(&self, src: &Path, label: &str, dest: &Path) -> Result<()> {
        self.log.stage(
            Stage::Install,
            format!("Installing {label} → {}", display_path(dest)),
        );
        let (src, dest) = (src.to_path_buf(), dest.to_path_buf());
        tokio::task::spawn_blocking(move || install::install(&src, &dest))
            .await
            .map_err(|err| Error::io("install task", std::io::Error::other(err)))?
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

/// Resolves `artifact` under `workdir`; `*`/`?` in the last segment pick the newest match.
pub fn resolve_artifact(workdir: &Path, artifact: &str) -> Result<PathBuf> {
    let path = workdir.join(artifact);
    let missing = |detail: String| {
        Error::Config(format!(
            "the build finished but install.artifact `{artifact}` {detail} (in {})",
            display_path(workdir)
        ))
    };
    let name = file_name(&path)?;
    if !name.contains(['*', '?']) {
        return if path.exists() {
            Ok(path)
        } else {
            Err(missing("does not exist".into()))
        };
    }
    let parent = path.parent().unwrap_or(workdir);
    let pattern = asset::glob_regex(&name)?;
    let newest = fs::read_dir(parent)
        .map_err(|_| missing("has no parent directory".into()))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| pattern.is_match(&entry.file_name().to_string_lossy()))
        .max_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    newest
        .map(|entry| entry.path())
        .ok_or_else(|| missing("matches nothing".into()))
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::InvalidInput(format!("{} has no file name", path.display())))
}

/// First 7 characters of a SHA, other refs unchanged.
pub fn short(reference: &str) -> String {
    let is_sha = reference.len() >= 12 && reference.chars().all(|c| c.is_ascii_hexdigit());
    if is_sha {
        reference.chars().take(7).collect()
    } else {
        reference.to_owned()
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024 * 10 && unit < UNITS.len() - 1 {
        value /= 1024;
        unit += 1;
    }
    format!("{value} {}", UNITS[unit])
}

/// Human-readable steps an update would take (for `--dry-run`).
pub fn plan_steps(
    app: &TrackedApp,
    settings: &Settings,
    paths: &Paths,
    target: &Candidate,
    asset: Option<&Asset>,
) -> Vec<String> {
    match &app.install {
        Install::Source(source) => {
            let dir = paths.source_dir(&app.id);
            let url = app.clone_url().unwrap_or_default();
            let fetch = if dir.join(".git").is_dir() {
                format!("fetch {url} in {}", display_path(&dir))
            } else {
                format!("clone {url} into {}", display_path(&dir))
            };
            let checkout = target.sha.as_deref().map_or_else(
                || format!("check out tag {}", target.id),
                |sha| format!("check out {}", short(sha)),
            );
            let workdir = source
                .workdir
                .as_deref()
                .map_or_else(|| dir.clone(), |sub| dir.join(sub));
            let install = match &source.artifact {
                Some(artifact) => {
                    let name = Path::new(artifact)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let dest =
                        install::resolve_dest(source.dest.as_deref(), app.kind, &name, settings);
                    format!("install {artifact} → {}", display_path(&dest))
                }
                None => "nothing to copy: the build command installs the program itself".to_owned(),
            };
            vec![
                fetch,
                checkout,
                format!("run in {}: {}", display_path(&workdir), source.build),
                install,
            ]
        }
        Install::Asset(spec) => {
            let Some(asset) = asset else {
                return vec!["no release asset matches; the update would fail".to_owned()];
            };
            let mut steps = vec![format!(
                "download {} ({})",
                asset.name,
                human_bytes(asset.size)
            )];
            if asset.digest.is_some() {
                steps.push("verify the SHA-256 digest published by GitHub".to_owned());
            }
            let repo_name = app.repo_id().map(|repo| repo.name).unwrap_or_default();
            let raw = archive::detect(&asset.name) == ArchiveKind::Raw;
            let step = match (app.kind, spec.binary.as_deref()) {
                (AppKind::Cli, None) if raw => {
                    let dest =
                        install::resolve_dest(spec.dest.as_deref(), app.kind, &repo_name, settings);
                    format!("install it as {}", display_path(&dest))
                }
                (AppKind::Cli, None) => format!(
                    "unpack it and install the executable named `{repo_name}` (or the only one inside) into {}",
                    spec.dest.as_deref().unwrap_or(&settings.bin_dir)
                ),
                (AppKind::App, None) => format!(
                    "unpack it and install the .app inside into {}",
                    spec.dest.as_deref().unwrap_or(&settings.apps_dir)
                ),
                (kind, Some(name)) => {
                    let payload =
                        if kind == AppKind::App && !name.to_ascii_lowercase().ends_with(".app") {
                            format!("{name}.app")
                        } else {
                            name.to_owned()
                        };
                    let dest =
                        install::resolve_dest(spec.dest.as_deref(), kind, &payload, settings);
                    format!("unpack it and install {payload} → {}", display_path(&dest))
                }
            };
            steps.push(step);
            steps
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn targets_parse() {
        assert_eq!(Target::parse("latest"), Target::Latest);
        assert_eq!(Target::parse(" next "), Target::Next);
        assert_eq!(Target::parse("v1.2.3"), Target::Ref("v1.2.3".into()));
        assert_eq!(
            serde_json::to_string(&Target::Ref("abc".into())).unwrap(),
            r#"{"ref":"abc"}"#
        );
        assert_eq!(
            serde_json::from_str::<Target>(r#""next""#).unwrap(),
            Target::Next
        );
    }

    #[test]
    fn artifacts_resolve_with_globs() {
        let dir = tempfile::tempdir().unwrap();
        let bundle_dir = dir.path().join("bundle/macos");
        fs::create_dir_all(bundle_dir.join("Demo.app")).unwrap();
        fs::write(dir.path().join("tool"), "").unwrap();
        assert_eq!(
            resolve_artifact(dir.path(), "tool").unwrap(),
            dir.path().join("tool")
        );
        assert_eq!(
            resolve_artifact(dir.path(), "bundle/macos/*.app").unwrap(),
            bundle_dir.join("Demo.app")
        );
        assert!(
            resolve_artifact(dir.path(), "missing")
                .unwrap_err()
                .to_string()
                .contains("does not exist")
        );
        assert!(resolve_artifact(dir.path(), "bundle/macos/*.zip").is_err());
    }

    #[test]
    fn run_log_writes_file_and_events() {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let sink = move |event: UpdateEvent| sink_events.lock().unwrap().push(event);
        let log = RunLog::create(dir.path(), "rg", &sink).unwrap();
        log.stage(Stage::Build, "Running `make`");
        log.line(LogStream::Stderr, "warning: x");
        log.progress(1, Some(10));
        log.progress(2, Some(10)); // throttled
        log.progress(10, Some(10)); // final, always sent

        let text = fs::read_to_string(log.path()).unwrap();
        assert!(text.contains("[headway] Running `make`"));
        assert!(text.contains("[err] warning: x"));
        let events = events.lock().unwrap();
        let progress = events
            .iter()
            .filter(|e| matches!(e, UpdateEvent::Progress { .. }))
            .count();
        assert_eq!(progress, 2);
        assert!(matches!(
            &events[0],
            UpdateEvent::Stage {
                stage: Stage::Build,
                ..
            }
        ));
        assert_eq!(latest_log(dir.path()).as_deref(), Some(log.path()));
    }

    #[test]
    fn old_logs_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..LOG_KEEP + 3 {
            fs::write(dir.path().join(format!("2026010{n:02}.log")), "").unwrap();
        }
        prune_logs(dir.path(), LOG_KEEP);
        assert_eq!(log_files(dir.path()).len(), LOG_KEEP);
        assert!(!dir.path().join("202601000.log").exists());
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(short("0123456789abcdef0123"), "0123456");
        assert_eq!(short("v1.2.3"), "v1.2.3");
        assert_eq!(short("refs/tags/v1"), "refs/tags/v1");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5120 KB");
        assert_eq!(human_bytes(50 * 1024 * 1024), "50 MB");
    }
}
