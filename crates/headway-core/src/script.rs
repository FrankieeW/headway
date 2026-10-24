//! A project's own updater (`claude update`, `rustup update`, `curl … | sh`): Headway
//! reads the versions with the commands the user gave and runs the update command.
//!
//! Without a `latest_command` there is no way to know whether an update is due: the
//! status is `unknown`, and an update simply runs the updater.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use chrono::Utc;

use crate::check::{Checked, InstalledSync};
use crate::error::{Error, Result};
use crate::events::Stage;
use crate::probe;
use crate::process;
use crate::shell::ShellEnv;
use crate::source::ScriptSource;
use crate::state::{Candidate, CheckResult, UpdateStatus};
use crate::update::{Installed, Job};
use crate::version::Version;

/// The id an update gets when no version can be read.
pub const LATEST: &str = "latest";

/// The installed version; exit 127 means the program is not installed.
pub async fn installed_version(
    cancel: &CancellationToken,
    env: &ShellEnv,
    source: &ScriptSource,
) -> Result<Option<String>> {
    let Some(command) = source.version_command.as_deref() else {
        return Ok(None);
    };
    let cmd = probe::shell(env, command);
    let out = probe::capture(cancel, &cmd).await?;
    if out.status == Some(127) {
        return Ok(None);
    }
    if !out.success() {
        return Err(probe::failed(&cmd, &out));
    }
    // Some tools print their version on stderr (`java -version`).
    probe::version_in(&out.stdout)
        .or_else(|| probe::version_in(&out.stderr))
        .map(Some)
        .ok_or_else(|| {
            Error::parse(
                format!("the output of `{command}`"),
                &"no version number in it",
            )
        })
}

/// The newest version from `latest_command`: a shell command, or an http(s) URL.
pub async fn latest_version(
    cancel: &CancellationToken,
    env: &ShellEnv,
    source: &ScriptSource,
) -> Result<Option<String>> {
    let Some(spec) = source.latest_command.as_deref().map(str::trim) else {
        return Ok(None);
    };
    let text = if spec.starts_with("https://") || spec.starts_with("http://") {
        fetch(cancel, spec).await?
    } else {
        probe::output(cancel, &probe::shell(env, spec))
            .await?
            .stdout
    };
    probe::version_in(&text).map(Some).ok_or_else(|| {
        Error::parse(
            format!("the output of `{spec}`"),
            &"no version number in it",
        )
    })
}

async fn fetch(cancel: &CancellationToken, url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("headway/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| Error::Network(err.to_string()))?;
    let response = tokio::select! {
        result = client.get(url).send() => result.map_err(|err| Error::Network(err.to_string()))?,
        () = cancel.cancelled() => return Err(Error::Cancelled),
    };
    if !response.status().is_success() {
        return Err(Error::Network(format!(
            "{url} answered HTTP {}",
            response.status()
        )));
    }
    tokio::select! {
        result = response.text() => result.map_err(|err| Error::Network(err.to_string())),
        () = cancel.cancelled() => Err(Error::Cancelled),
    }
}

fn candidate(version: &str) -> Candidate {
    Candidate {
        id: version.to_owned(),
        sha: None,
        title: version.to_owned(),
        body: None,
        author: None,
        date: None,
        url: None,
        prerelease: Version::parse(version).is_some_and(|parsed| parsed.is_prerelease()),
    }
}

pub async fn check(
    cancel: &CancellationToken,
    env: &ShellEnv,
    source: &ScriptSource,
) -> Result<Checked> {
    let installed = installed_version(cancel, env, source).await?;
    let latest = latest_version(cancel, env, source).await?;
    let readable = source.version_command.is_some();
    let (status, pending) = match (&installed, &latest) {
        (_, None) => (UpdateStatus::Unknown, Vec::new()),
        (None, Some(_)) if !readable => (UpdateStatus::Unknown, Vec::new()),
        (None, Some(latest)) => (UpdateStatus::NotInstalled, vec![candidate(latest)]),
        (Some(current), Some(latest)) => {
            let newer = match (Version::parse(current), Version::parse(latest)) {
                (Some(current), Some(latest)) => latest > current,
                _ => current != latest,
            };
            if newer {
                (UpdateStatus::UpdateAvailable, vec![candidate(latest)])
            } else {
                (UpdateStatus::UpToDate, Vec::new())
            }
        }
    };
    let pending_total = u32::from(status == UpdateStatus::UpdateAvailable);
    Ok(Checked {
        result: CheckResult {
            checked_at: Utc::now(),
            status,
            branch: None,
            latest: latest.as_deref().map(candidate),
            pending,
            pending_total,
        },
        installed: InstalledSync::Detected(installed),
    })
}

/// What an update targets when the newest version can't be read.
pub fn unknown_target() -> Candidate {
    Candidate {
        title: "the newest version".to_owned(),
        ..candidate(LATEST)
    }
}

pub async fn install(job: &Job<'_>, source: &ScriptSource) -> Result<Installed> {
    let cancel = job.cancel;
    let before = installed_version(cancel, job.env, source).await?;
    let cmd = probe::shell(job.env, &source.update_command);
    job.log.stage(
        Stage::Install,
        format!("Running `{}`", source.update_command),
    );
    process::run(&cmd, job.cancel, &mut job.output()).await?;
    let after = installed_version(cancel, job.env, source).await?;
    if source.version_command.is_some() && after.is_none() {
        return Err(Error::Config(
            "the update command finished but the version command still reports the program missing"
                .into(),
        ));
    }
    if let (Some(before), Some(after)) = (&before, &after)
        && before == after
    {
        job.log.info(&format!(
            "Still {after} after the update: it was already current."
        ));
    }
    Ok(Installed {
        sha: None,
        version: after,
        path: None,
    })
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn source(version: Option<&str>, latest: Option<&str>) -> ScriptSource {
        ScriptSource {
            update_command: "true".into(),
            version_command: version.map(str::to_owned),
            latest_command: latest.map(str::to_owned),
        }
    }

    async fn status(source: &ScriptSource) -> (UpdateStatus, InstalledSync) {
        let checked = check(&CancellationToken::new(), &ShellEnv::current(), source)
            .await
            .unwrap();
        (checked.result.status, checked.installed)
    }

    #[tokio::test]
    async fn compares_the_two_versions() {
        let behind = source(Some("echo tool 1.2.0"), Some("echo 1.3.0"));
        assert_eq!(
            status(&behind).await,
            (
                UpdateStatus::UpdateAvailable,
                InstalledSync::Detected(Some("1.2.0".into()))
            )
        );
        let current = source(Some("echo v1.3.0"), Some("echo 1.3.0"));
        assert_eq!(status(&current).await.0, UpdateStatus::UpToDate);
        let missing = source(Some("exit 127"), Some("echo 1.3.0"));
        assert_eq!(
            status(&missing).await,
            (UpdateStatus::NotInstalled, InstalledSync::Detected(None))
        );
    }

    #[tokio::test]
    async fn without_a_latest_version_the_status_is_unknown() {
        assert_eq!(
            status(&source(Some("echo 1.0.0"), None)).await.0,
            UpdateStatus::Unknown
        );
        assert_eq!(
            status(&source(None, Some("echo 2.0.0"))).await,
            (UpdateStatus::Unknown, InstalledSync::Detected(None))
        );
        let broken = source(None, Some("echo no version here"));
        assert!(
            check(&CancellationToken::new(), &ShellEnv::current(), &broken)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn latest_versions_can_come_from_a_url() {
        let server = MockServer::start().await;
        Mock::given(path("/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"tag_name\":\"v4.5.6\"}"))
            .mount(&server)
            .await;
        let from_url = source(
            Some("echo 4.5.6"),
            Some(&format!("{}/latest", server.uri())),
        );
        let checked = check(&CancellationToken::new(), &ShellEnv::current(), &from_url)
            .await
            .unwrap();
        assert_eq!(checked.result.latest.unwrap().id, "4.5.6");
        assert_eq!(checked.result.status, UpdateStatus::UpToDate);
    }
}
