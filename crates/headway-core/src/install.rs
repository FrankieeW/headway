//! Putting a built or downloaded payload in place.
//!
//! The new copy is staged next to the destination and renamed over it, so a
//! running binary is never overwritten in place (macOS kills processes whose
//! signed code changes under them) and a failed copy leaves the old version intact.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{AppKind, Settings};
use crate::error::{Error, Result};
use crate::paths::expand_tilde;

mod journal;
pub(crate) use journal::{Transaction, reconcile, sync_dir, sync_tree};

/// `dest` from the config, or `bin_dir`/`apps_dir` joined with `file_name`.
pub fn resolve_dest(
    configured: Option<&str>,
    kind: AppKind,
    file_name: &str,
    settings: &Settings,
) -> PathBuf {
    configured.map_or_else(
        || {
            let dir = match kind {
                AppKind::Cli => &settings.bin_dir,
                AppKind::App => &settings.apps_dir,
            };
            expand_tilde(dir).join(file_name)
        },
        expand_tilde,
    )
}

/// Copies `src` (a file or a bundle directory) to `dest`, replacing what is there.
pub fn install(src: &Path, dest: &Path) -> Result<()> {
    reconcile(dest, false)?;
    Transaction::begin(src, dest)?.commit()
}

pub(crate) fn validate(src: &Path, dest: &Path) -> Result<()> {
    // Only a bundle replaces a bundle. A plain directory at `dest` is almost certainly a
    // mistyped destination (`--dest ~/.local/bin`) and must never be swapped out.
    if src.is_dir() && !(is_bundle(src) && is_bundle(dest)) {
        return Err(Error::InvalidInput(
            "only .app/.framework bundle directories can be installed".into(),
        ));
    }
    let existing_dir = dest.is_dir() && !dest.is_symlink();
    if existing_dir && !(src.is_dir() && is_bundle(src) && is_bundle(dest)) {
        let example = dest.join(src.file_name().unwrap_or_default());
        return Err(Error::InvalidInput(format!(
            "{} is an existing directory and is never replaced; set `dest` to the full path of the \
             installed {}, e.g. {}",
            dest.display(),
            if src.is_dir() { "bundle" } else { "file" },
            example.display()
        )));
    }

    Ok(())
}

/// `.app` and `.framework` directories: the only directories Headway installs or replaces.
pub fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app") || ext.eq_ignore_ascii_case("framework"))
}

pub(crate) fn copy_payload(src: &Path, dest: &Path) -> Result<()> {
    if src.is_dir() {
        copy_dir(src, dest)
    } else {
        fs::copy(src, dest).map_err(|err| Error::io_at("copy", dest, err))?;
        Ok(())
    }
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        // Keeps symlinks, extended attributes and code signatures.
        ("/usr/bin/ditto", &[])
    } else {
        ("cp", &["-a"])
    };
    crate::process::Cmd::new(program, &crate::shell::ShellEnv::current())
        .args(args.iter().copied())
        .arg(src.display().to_string())
        .arg(dest.display().to_string())
        .run_blocking()
}

pub(crate) fn remove_any(path: &Path) -> Result<()> {
    let result = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    };
    result.map_err(|err| Error::io_at("remove", path, err))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn default_destinations() {
        let settings = Settings {
            bin_dir: "/opt/bin".into(),
            apps_dir: "/Apps".into(),
            ..Settings::default()
        };
        assert_eq!(
            resolve_dest(None, AppKind::Cli, "rg", &settings),
            PathBuf::from("/opt/bin/rg")
        );
        assert_eq!(
            resolve_dest(None, AppKind::App, "Zed.app", &settings),
            PathBuf::from("/Apps/Zed.app")
        );
        assert_eq!(
            resolve_dest(
                Some("/usr/local/bin/ripgrep"),
                AppKind::Cli,
                "rg",
                &settings
            ),
            PathBuf::from("/usr/local/bin/ripgrep")
        );
    }

    #[test]
    fn installs_and_replaces_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("rg");
        fs::write(&src, "v1").unwrap();
        let dest = dir.path().join("bin/rg");
        install(&src, &dest).unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "v1");
        assert_eq!(
            fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
            0o755
        );

        fs::write(&src, "v2").unwrap();
        install(&src, &dest).unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "v2");
        let leftovers: Vec<_> = fs::read_dir(dir.path().join("bin")).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no staging files left behind");
    }

    #[test]
    fn never_replaces_plain_directories() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("rg");
        fs::write(&src, "binary").unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("fd"), "keep me").unwrap();

        let err = install(&src, &bin).unwrap_err();
        assert!(matches!(&err, Error::InvalidInput(msg) if msg.contains("never replaced")));
        assert_eq!(fs::read_to_string(bin.join("fd")).unwrap(), "keep me");

        // A bundle payload may not replace a plain directory either.
        let bundle = dir.path().join("build/Demo.app");
        fs::create_dir_all(&bundle).unwrap();
        assert!(install(&bundle, &bin).is_err());
        assert!(bin.join("fd").exists());
    }

    #[test]
    fn replaces_a_symlink_with_a_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("build/Demo.app");
        fs::create_dir_all(src.join("Contents")).unwrap();
        let elsewhere = dir.path().join("old/Demo.app");
        fs::create_dir_all(&elsewhere).unwrap();
        let dest = dir.path().join("Applications/Demo.app");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &dest).unwrap();

        install(&src, &dest).unwrap();
        assert!(dest.is_dir() && !dest.is_symlink());
        assert!(elsewhere.exists(), "the link target is left alone");
    }

    #[test]
    fn replaces_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("build/Demo.app");
        fs::create_dir_all(src.join("Contents/MacOS")).unwrap();
        fs::write(src.join("Contents/MacOS/Demo"), "new").unwrap();
        let dest = dir.path().join("Applications/Demo.app");
        fs::create_dir_all(dest.join("Contents/Resources")).unwrap();
        fs::write(dest.join("Contents/Resources/old.txt"), "old").unwrap();

        install(&src, &dest).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("Contents/MacOS/Demo")).unwrap(),
            "new"
        );
        assert!(!dest.join("Contents/Resources/old.txt").exists());
        assert!(
            !dir.path()
                .join("Applications/.Demo.app.headway-old")
                .exists()
        );
    }
}
