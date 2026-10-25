use super::ArchiveKind;
use crate::error::{Error, Result};
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;
use std::fs;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub(super) const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRIES: u64 = 100_000;

/// Conservative preflight, before materializing any decoded archive content.
/// The system decoder streams logical bytes (including sparse holes) to a counting
/// sink, then ditto preserves ZIP AppleDouble/signatures in the extraction pass.
/// This is a resource guard, not a sandbox against parser discrepancies.
pub(super) async fn preflight(
    file: &Path,
    kind: ArchiveKind,
    env: &ShellEnv,
    cancel: &CancellationToken,
) -> Result<()> {
    preflight_with_limits(file, kind, env, cancel, MAX_BYTES, MAX_ENTRIES).await
}

async fn preflight_with_limits(
    file: &Path,
    kind: ArchiveKind,
    env: &ShellEnv,
    cancel: &CancellationToken,
    max_bytes: u64,
    max_entries: u64,
) -> Result<()> {
    let file = file.display().to_string();
    let (listing, content) = if kind == ArchiveKind::Zip && !cfg!(target_os = "macos") {
        (
            Cmd::new("/usr/bin/unzip", env).args(["-Z", "-1", &file]),
            Cmd::new("/usr/bin/unzip", env).args(["-p", &file]),
        )
    } else {
        (
            Cmd::new("/usr/bin/tar", env).args(["-tf", &file]),
            Cmd::new("/usr/bin/tar", env).args(["-xOf", &file]),
        )
    };
    process::measure(&listing, cancel, 64 * 1024 * 1024, Some(max_entries)).await?;
    process::measure(&content, cancel, max_bytes, None).await
}

pub(crate) fn validate_tree(path: &Path) -> Result<()> {
    let mut entries = 0;
    let mut bytes = 0;
    scan(path, &mut entries, &mut bytes, 0)
}

fn scan(path: &Path, entries: &mut u64, bytes: &mut u64, depth: usize) -> Result<()> {
    if depth > 128 {
        return Err(Error::NoAsset(
            "payload exceeds the directory-depth budget".into(),
        ));
    }
    *entries += 1;
    let meta =
        fs::symlink_metadata(path).map_err(|error| Error::io_at("inspect payload", path, error))?;
    *bytes = bytes.saturating_add(meta.len());
    if *entries > MAX_ENTRIES || *bytes > MAX_BYTES {
        return Err(Error::NoAsset(
            "payload exceeds the entry-count/byte budget".into(),
        ));
    }
    if meta.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|error| Error::io_at("inspect payload", path, error))?
        {
            scan(
                &entry
                    .map_err(|error| Error::io("read payload entry", error))?
                    .path(),
                entries,
                bytes,
                depth + 1,
            )?;
        }
    } else if !meta.is_file() && !meta.is_symlink() {
        return Err(Error::NoAsset(
            "payload contains an unsupported special file".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    #[tokio::test]
    async fn compressed_bomb_and_entry_overflow_fail_before_extraction() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("zeros"), vec![0; 512 * 1024]).unwrap();
        fs::write(dir.path().join("second"), "x").unwrap();
        let archive = dir.path().join("test.tar.gz");
        assert!(
            Command::new("/usr/bin/tar")
                .args(["-czf"])
                .arg(&archive)
                .arg("-C")
                .arg(dir.path())
                .args(["zeros", "second"])
                .status()
                .unwrap()
                .success()
        );
        let env = ShellEnv::current();
        let cancel = CancellationToken::new();
        let result =
            preflight_with_limits(&archive, ArchiveKind::Tar, &env, &cancel, 1024, 100).await;
        assert!(matches!(result, Err(Error::NoAsset(_))));
        let result =
            preflight_with_limits(&archive, ArchiveKind::Tar, &env, &cancel, MAX_BYTES, 1).await;
        assert!(matches!(result, Err(Error::NoAsset(_))));
        preflight_with_limits(&archive, ArchiveKind::Tar, &env, &cancel, MAX_BYTES, 100)
            .await
            .unwrap();
    }
}
