//! Bounded, read-only Homebrew queries with validation before process launch.
use super::metadata::{self, PackageMeta};
use crate::{
    CancellationToken, Error, Result, probe, process::Cmd, shell::ShellEnv, source::is_brew_name,
};

pub(super) fn command(env: &ShellEnv) -> Cmd {
    Cmd::new("brew", env)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
}

pub(super) async fn info(
    cancel: &CancellationToken,
    env: &ShellEnv,
    names: &[String],
    cask: bool,
) -> Result<Vec<PackageMeta>> {
    if names.is_empty() || names.iter().any(|name| !is_brew_name(name)) {
        return Err(Error::InvalidInput("invalid Homebrew package name".into()));
    }
    let flag = if cask { "--cask" } else { "--formula" };
    let cmd = command(env).args(["info", "--json=v2", flag]).args(names);
    let out = probe::capture(cancel, &cmd).await?;
    if !out.success() {
        if out.stderr.contains("No available") || out.stderr.contains("No formulae or casks found")
        {
            return Err(missing(&names.join(", "), cask));
        }
        return Err(probe::failed(&cmd, &out));
    }
    let packages = metadata::parse(&out.stdout)?;
    Ok(packages.into_iter().filter(|p| p.cask == cask).collect())
}

pub(super) fn missing(name: &str, cask: bool) -> Error {
    Error::RemoteNotFound(format!(
        "Homebrew {} `{name}`",
        if cask { "cask" } else { "formula" }
    ))
}

pub(super) async fn installed(
    cancel: &CancellationToken,
    env: &ShellEnv,
) -> Result<Vec<PackageMeta>> {
    let cmd = command(env).args(["info", "--installed", "--json=v2"]);
    let out = probe::capture(cancel, &cmd).await?;
    if !out.success() {
        return Err(probe::failed(&cmd, &out));
    }
    metadata::parse(&out.stdout)
}
