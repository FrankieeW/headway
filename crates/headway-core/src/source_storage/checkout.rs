use std::fs;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

use super::Location;
use super::ownership::checked;
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;
use crate::{Error, Result};

pub(crate) struct CheckoutContext<'a> {
    pub url: &'a str,
    pub env: &'a ShellEnv,
    pub cancel: &'a CancellationToken,
}

pub(crate) struct SourceCheckout {
    path: PathBuf,
    _temporary: Option<tempfile::TempDir>,
}

impl SourceCheckout {
    pub async fn prepare(location: &Location, context: &CheckoutContext<'_>) -> Result<Self> {
        if context.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if !location.keep {
            fs::create_dir_all(&location.base)
                .map_err(|error| Error::io_at("create", &location.base, error))?;
            checked(&location.base)?;
            let temporary = tempfile::Builder::new()
                .prefix(&format!(
                    ".headway-{}-",
                    location
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ))
                .tempdir_in(&location.base)
                .map_err(|error| Error::io_at("create temporary source", &location.base, error))?;
            return Ok(Self {
                path: temporary.path().join("checkout"),
                _temporary: Some(temporary),
            });
        }
        if !location.owned()? {
            if !location.empty()? {
                if location.custom_root {
                    return Err(location.unowned());
                }
                validate_legacy(location, context).await?;
            }
            location.claim()?;
        }
        checked(&location.path)?;
        validate_git_directory(&location.path)?;
        Ok(Self {
            path: location.path.clone(),
            _temporary: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub(super) fn validate_git_directory(path: &Path) -> Result<()> {
    let git = path.join(".git");
    if git.exists() || git.is_symlink() {
        checked(&git)?;
        let config = git.join("config");
        if config.is_symlink() {
            return Err(Error::InvalidInput(
                "source git config cannot be a symlink".into(),
            ));
        }
    }
    Ok(())
}

async fn validate_legacy(location: &Location, context: &CheckoutContext<'_>) -> Result<()> {
    validate_git_directory(&location.path)?;
    if !location.path.join(".git").is_dir() {
        return Err(location.unowned());
    }
    let origin = read_git(location, context, &["config", "--get", "remote.origin.url"]).await?;
    if !same_origin(&origin, context.url) {
        return Err(location.unowned());
    }
    let top = read_git(location, context, &["rev-parse", "--show-toplevel"]).await?;
    if top != location.path.display().to_string() {
        return Err(location.unowned());
    }
    Ok(())
}

async fn read_git(
    location: &Location,
    context: &CheckoutContext<'_>,
    args: &[&str],
) -> Result<String> {
    let command = Cmd::new("git", context.env)
        .arg("-C")
        .arg(location.path.display().to_string())
        .args(args.iter().copied());
    let output = tokio::select! {
        output = process::capture(&command) => output?,
        () = context.cancel.cancelled() => return Err(Error::Cancelled),
    };
    if !output.success() {
        return Err(location.unowned());
    }
    Ok(output.stdout.trim().to_owned())
}

fn same_origin(left: &str, right: &str) -> bool {
    left == right
        || matches!((github_repo(left), github_repo(right)),
        (Some(left), Some(right)) if left == right)
}

/// Canonicalize only clone endpoints with an exact github.com authority.
fn github_repo(remote: &str) -> Option<String> {
    let path = if let Some((host, path)) = remote
        .strip_prefix("git@")
        .and_then(|scp| scp.split_once(':'))
    {
        if !host.eq_ignore_ascii_case("github.com") {
            return None;
        }
        path.to_owned()
    } else {
        let url = reqwest::Url::parse(remote).ok()?;
        let port = match url.scheme() {
            "http" => 80,
            "https" => 443,
            "ssh" if url.username() == "git" => 22,
            "git" if url.username().is_empty() => 9418,
            _ => return None,
        };
        if !url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
            || url.port().is_some_and(|actual| actual != port)
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        url.path().strip_prefix('/')?.to_owned()
    };
    let path = path.strip_suffix('/').unwrap_or(&path);
    let mut parts = path.split('/');
    let (Some(_), Some(_), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    // Host and exact owner/name path shape are proven before RepoId's permissive parser.
    Some(
        crate::repo::RepoId::parse(&path.to_ascii_lowercase())
            .ok()?
            .full_name(),
    )
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod remote_tests;
