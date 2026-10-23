//! Unpacking release assets and finding the binary or `.app` inside.

use std::fs;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::events::LogStream;
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    /// Any tarball `tar` can read (gz, xz, bz2, zst or plain).
    Tar,
    Zip,
    Dmg,
    /// A bare executable.
    Raw,
}

pub fn detect(name: &str) -> ArchiveKind {
    let name = name.to_ascii_lowercase();
    let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| name.ends_with(suffix));
    if ends(&[
        ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz", ".tbz2", ".tar.zst", ".tzst",
        ".tar",
    ]) {
        ArchiveKind::Tar
    } else if ends(&[".zip"]) {
        ArchiveKind::Zip
    } else if ends(&[".dmg"]) {
        ArchiveKind::Dmg
    } else {
        ArchiveKind::Raw
    }
}

/// An unpacked asset. Disk images stay mounted until this is dropped.
#[derive(Debug)]
pub struct Unpacked {
    pub root: PathBuf,
    mount: Option<PathBuf>,
}

impl Drop for Unpacked {
    fn drop(&mut self) {
        if let Some(mount) = &self.mount {
            let _ = std::process::Command::new("/usr/bin/hdiutil")
                .args(["detach", "-quiet", "-force"])
                .arg(mount)
                .status();
        }
    }
}

/// Unpacks `file` into `dest` (or mounts it there, for disk images).
pub async fn unpack(
    file: &Path,
    kind: ArchiveKind,
    dest: &Path,
    env: &ShellEnv,
    cancel: &CancellationToken,
    on_line: &mut (dyn FnMut(LogStream, &str) + Send),
) -> Result<Unpacked> {
    fs::create_dir_all(dest).map_err(|err| Error::io_at("create", dest, err))?;
    let file_arg = file.display().to_string();
    let dest_arg = dest.display().to_string();
    let unpacked = |root: PathBuf| Unpacked { root, mount: None };
    match kind {
        ArchiveKind::Tar => {
            let cmd = Cmd::new("tar", env).args(["-xf", &file_arg, "-C", &dest_arg]);
            process::run(&cmd, cancel, on_line).await?;
            Ok(unpacked(dest.to_path_buf()))
        }
        ArchiveKind::Zip => {
            // `ditto` keeps symlinks, extended attributes and code signatures inside app bundles.
            let cmd = if cfg!(target_os = "macos") {
                Cmd::new("/usr/bin/ditto", env).args(["-x", "-k", &file_arg, &dest_arg])
            } else {
                Cmd::new("unzip", env).args(["-q", &file_arg, "-d", &dest_arg])
            };
            process::run(&cmd, cancel, on_line).await?;
            Ok(unpacked(dest.to_path_buf()))
        }
        ArchiveKind::Dmg => {
            let mount = dest.join("volume");
            fs::create_dir_all(&mount).map_err(|err| Error::io_at("create", &mount, err))?;
            let mount_arg = mount.display().to_string();
            let cmd = Cmd::new("/usr/bin/hdiutil", env).args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
                &mount_arg,
                &file_arg,
            ]);
            process::run(&cmd, cancel, on_line).await?;
            Ok(Unpacked {
                root: mount.clone(),
                mount: Some(mount),
            })
        }
        ArchiveKind::Raw => {
            let name = file.file_name().ok_or_else(|| {
                Error::InvalidInput(format!("{} has no file name", file.display()))
            })?;
            let target = dest.join(name);
            fs::copy(file, &target).map_err(|err| Error::io_at("copy", &target, err))?;
            set_executable(&target)?;
            Ok(unpacked(dest.to_path_buf()))
        }
    }
}

pub fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|err| Error::io_at("chmod", path, err))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app") || ext.eq_ignore_ascii_case("framework"))
}

/// Regular files under `root`, without following symlinks or entering bundles.
fn files(root: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(std::result::Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_file() => vec![path],
                Ok(kind) if kind.is_dir() && depth > 0 && !is_bundle(&path) => {
                    files(&path, depth - 1)
                }
                _ => Vec::new(),
            }
        })
        .collect()
}

/// `.app` bundles under `root` (not nested inside other bundles, symlinks not followed).
fn bundles(root: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(std::result::Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if !is_dir {
                Vec::new()
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
            {
                vec![path]
            } else if depth > 0 {
                bundles(&path, depth - 1)
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn names(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Finds the CLI binary: a file called `name` (executable preferred), or the only executable.
pub fn find_binary(root: &Path, name: &str) -> Result<PathBuf> {
    let all = files(root, 6);
    let named: Vec<&PathBuf> = all
        .iter()
        .filter(|path| path.file_name().is_some_and(|file| file == name))
        .collect();
    if let Some(found) = named
        .iter()
        .find(|path| is_executable(path))
        .or_else(|| named.first())
    {
        return Ok((*found).clone());
    }
    let libraries = [".dylib", ".so", ".a", ".sh", ".py", ".rb", ".js"];
    let executables: Vec<PathBuf> = all
        .into_iter()
        .filter(|path| {
            let file = path.to_string_lossy().to_ascii_lowercase();
            is_executable(path) && !libraries.iter().any(|ext| file.ends_with(ext))
        })
        .collect();
    match executables.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(Error::NoAsset(format!(
            "the asset contains no executable named `{name}`"
        ))),
        many => Err(Error::NoAsset(format!(
            "no executable named `{name}`; the asset has {}; set `binary` to pick one",
            names(many)
        ))),
    }
}

/// Finds the `.app` bundle: the one called `name` (with or without `.app`), or the only one.
pub fn find_app_bundle(root: &Path, name: Option<&str>) -> Result<PathBuf> {
    let found = bundles(root, 3);
    if let Some(name) = name {
        let wanted = if name.to_ascii_lowercase().ends_with(".app") {
            name.to_owned()
        } else {
            format!("{name}.app")
        };
        return found
            .iter()
            .find(|path| {
                path.file_name()
                    .is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(&wanted))
            })
            .cloned()
            .ok_or_else(|| {
                Error::NoAsset(format!(
                    "`{wanted}` not found in the asset (found: {})",
                    names(&found)
                ))
            });
    }
    match found.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(Error::NoAsset("the asset contains no .app bundle".into())),
        many => Err(Error::NoAsset(format!(
            "the asset has several apps ({}); set `binary` to pick one",
            names(many)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_exec(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "#!/bin/sh\necho hi\n").unwrap();
        set_executable(path).unwrap();
    }

    #[test]
    fn detects_archive_kinds() {
        assert_eq!(detect("rg-aarch64-apple-darwin.tar.gz"), ArchiveKind::Tar);
        assert_eq!(detect("x.TGZ"), ArchiveKind::Tar);
        assert_eq!(detect("x.tar.zst"), ArchiveKind::Tar);
        assert_eq!(detect("App.app.zip"), ArchiveKind::Zip);
        assert_eq!(detect("App.dmg"), ArchiveKind::Dmg);
        assert_eq!(detect("jq-macos-arm64"), ArchiveKind::Raw);
    }

    #[test]
    fn finds_named_or_single_binary() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_exec(&root.join("ripgrep-14/rg"));
        fs::write(root.join("ripgrep-14/README.md"), "docs").unwrap();
        assert_eq!(find_binary(root, "rg").unwrap(), root.join("ripgrep-14/rg"));
        // Repo name differs from the binary name: the only executable wins.
        assert_eq!(
            find_binary(root, "ripgrep").unwrap(),
            root.join("ripgrep-14/rg")
        );

        write_exec(&root.join("ripgrep-14/rga"));
        let err = find_binary(root, "ripgrep").unwrap_err();
        assert!(
            matches!(&err, Error::NoAsset(msg) if msg.contains("rg") && msg.contains("set `binary`"))
        );
    }

    #[test]
    fn ignores_files_inside_bundles_and_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_exec(&root.join("Tool.app/Contents/MacOS/Tool"));
        write_exec(&root.join("lib/libtool.dylib"));
        write_exec(&root.join("bin/tool"));
        assert_eq!(find_binary(root, "missing").unwrap(), root.join("bin/tool"));
    }

    #[test]
    fn finds_app_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_exec(&root.join("Zed.app/Contents/MacOS/zed"));
        assert_eq!(find_app_bundle(root, None).unwrap(), root.join("Zed.app"));
        assert_eq!(
            find_app_bundle(root, Some("zed")).unwrap(),
            root.join("Zed.app")
        );
        write_exec(&root.join("nested/Other.app/Contents/MacOS/other"));
        assert!(find_app_bundle(root, None).is_err());
        assert_eq!(
            find_app_bundle(root, Some("Other.app")).unwrap(),
            root.join("nested/Other.app")
        );
        assert!(find_app_bundle(root, Some("Nope")).is_err());
    }

    #[tokio::test]
    async fn unpacks_tarballs_and_raw_binaries() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        write_exec(&src.join("tool-1.0/tool"));
        let tarball = dir.path().join("tool.tar.gz");
        let status = std::process::Command::new("tar")
            .args([
                "-czf",
                &tarball.display().to_string(),
                "-C",
                &src.display().to_string(),
                "tool-1.0",
            ])
            .status()
            .unwrap();
        assert!(status.success());

        let env = ShellEnv::current();
        let cancel = CancellationToken::new();
        let out = dir.path().join("out");
        let unpacked = unpack(
            &tarball,
            ArchiveKind::Tar,
            &out,
            &env,
            &cancel,
            &mut |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(
            find_binary(&unpacked.root, "tool").unwrap(),
            out.join("tool-1.0/tool")
        );

        let raw_out = dir.path().join("raw");
        let raw = unpack(
            &src.join("tool-1.0/tool"),
            ArchiveKind::Raw,
            &raw_out,
            &env,
            &cancel,
            &mut |_, _| {},
        )
        .await
        .unwrap();
        assert!(is_executable(&raw.root.join("tool")));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn unpacks_zips_with_ditto() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Demo.app");
        write_exec(&app.join("Contents/MacOS/Demo"));
        let zip = dir.path().join("Demo.app.zip");
        let status = std::process::Command::new("/usr/bin/ditto")
            .args([
                "-c",
                "-k",
                "--keepParent",
                &app.display().to_string(),
                &zip.display().to_string(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
        let out = dir.path().join("out");
        let unpacked = unpack(
            &zip,
            ArchiveKind::Zip,
            &out,
            &ShellEnv::current(),
            &CancellationToken::new(),
            &mut |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(
            find_app_bundle(&unpacked.root, None).unwrap(),
            out.join("Demo.app")
        );
    }
}
