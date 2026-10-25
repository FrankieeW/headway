use crate::error::{Error, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn validate_pattern(artifact: &str) -> Result<()> {
    let path = Path::new(artifact);
    if artifact.is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(Error::Config(
            "install.artifact must remain inside the build workdir after substitution".into(),
        ));
    }
    Ok(())
}

/// An ambiguous glob is not evidence that the newest-mtime result belongs to this
/// build. Require an exact choice instead, without rejecting incremental output.
pub fn resolve_artifact(workdir: &Path, artifact: &str) -> Result<PathBuf> {
    validate_pattern(artifact)?;
    let path = workdir.join(artifact);
    let missing = |detail: &str| {
        Error::Config(format!(
            "the build finished but install.artifact `{artifact}` {detail} (in {})",
            workdir.display()
        ))
    };
    let name = super::file_name(&path)?;
    let chosen = if name.contains(['*', '?']) {
        let pattern = crate::asset::glob_regex(&name)?;
        let parent = path
            .parent()
            .ok_or_else(|| missing("has no parent directory"))?;
        let mut matches = fs::read_dir(parent)
            .map_err(|_| missing("has no parent directory"))?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| Error::io("read artifact candidates", error))?
            .into_iter()
            .filter(|entry| pattern.is_match(&entry.file_name().to_string_lossy()))
            .map(|entry| entry.path());
        let chosen = matches.next().ok_or_else(|| missing("matches nothing"))?;
        if matches.next().is_some() {
            return Err(missing(
                "matches several outputs; configure an exact artifact path",
            ));
        }
        chosen
    } else {
        if !path.exists() {
            return Err(missing("does not exist"));
        }
        path
    };
    let root = workdir
        .canonicalize()
        .map_err(|error| Error::io_at("resolve workdir", workdir, error))?;
    let actual = chosen
        .canonicalize()
        .map_err(|error| Error::io_at("resolve artifact", &chosen, error))?;
    if !actual.starts_with(&root) || actual == root {
        return Err(missing(
            "escapes the build workdir (including via a symlink)",
        ));
    }
    Ok(chosen)
}

pub(super) async fn before_build(
    dir: &Path,
    workdir: &Path,
    artifact: Option<&str>,
) -> Result<Option<String>> {
    let root = dir
        .canonicalize()
        .map_err(|error| Error::io_at("resolve checkout", dir, error))?;
    let actual = workdir
        .canonicalize()
        .map_err(|error| Error::io_at("resolve workdir", workdir, error))?;
    if !actual.starts_with(root) {
        return Err(Error::Config(
            "install.workdir escapes the checkout via a symlink".into(),
        ));
    }
    if let Some(artifact) = artifact {
        validate_pattern(artifact)?;
    }
    match artifact.and_then(|artifact| resolve_artifact(workdir, artifact).ok()) {
        Some(path)
            if path
                .metadata()
                .is_ok_and(|meta| meta.is_file() && meta.len() <= 128 * 1024 * 1024) =>
        {
            Ok(Some(fingerprint(path).await?))
        }
        _ => Ok(None),
    }
}

#[derive(serde::Deserialize)]
struct Version {
    ok: bool,
    data: crate::build_info::BuildInfo,
}

pub(super) async fn fingerprint(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || crate::recovery::payload_digest(&path))
        .await
        .map_err(|error| Error::io("artifact fingerprint worker", std::io::Error::other(error)))?
}

pub(super) async fn verify_self_build(
    output: &Path,
    sha: &str,
    repo: &str,
    env: &crate::shell::ShellEnv,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<()> {
    let cli = if output.is_dir() {
        output.join("Contents/MacOS/headway")
    } else {
        output.to_owned()
    };
    let command = crate::process::Cmd::new(cli.display().to_string(), env)
        .args(["--json", "version"])
        .timeout(std::time::Duration::from_secs(30));
    // Only the explicit self-hosting recipe's output is executed by this check.
    let captured = crate::process::capture_with_cancel(cancel, &command, 16 * 1024 * 1024).await?;
    let version: Version = serde_json::from_str(&captured.stdout)
        .map_err(|error| Error::parse("self-built artifact provenance", &error))?;
    if !captured.success()
        || !version.ok
        || version.data.dirty
        || version.data.commit.as_deref() != Some(sha)
        || !version
            .data
            .repository
            .as_deref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(repo))
    {
        return Err(Error::InvalidInput("self-built artifact does not identify the clean selected commit and repository; refusing a stale/dirty baseline".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_glob_and_path_or_symlink_escape() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("one.bin"), "one").unwrap();
        fs::write(root.path().join("two.bin"), "two").unwrap();
        assert!(
            resolve_artifact(root.path(), "*.bin")
                .unwrap_err()
                .to_string()
                .contains("several")
        );
        assert!(resolve_artifact(root.path(), "../secret").is_err());
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        assert!(resolve_artifact(root.path(), "escape").is_err());
    }
}
