//! Unpacking release assets and finding the binary or `.app` inside.

use std::fs;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::events::LogStream;
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;

mod budget;
pub(crate) use budget::validate_tree;

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

impl Unpacked {
    pub async fn detach(&mut self, env: &ShellEnv) -> Result<()> {
        let Some(mount) = &self.mount else {
            return Ok(());
        };
        process::run(
            &detach_command(mount, env),
            &CancellationToken::new(),
            &mut |_, _| {},
        )
        .await?;
        self.mount = None;
        Ok(())
    }
}

fn detach_command(mount: &Path, env: &ShellEnv) -> Cmd {
    Cmd::new("/usr/bin/hdiutil", env)
        .args(["detach", "-quiet", "-force"])
        .arg(mount.display().to_string())
        .timeout(std::time::Duration::from_secs(30))
}

impl Drop for Unpacked {
    fn drop(&mut self) {
        if let Some(mount) = self.mount.take() {
            // Drop cannot await, and must not block a Tokio worker. Explicit
            // detach is the normal path; this bounded worker is a safety net.
            std::thread::spawn(move || {
                if let Err(error) = detach_command(&mount, &ShellEnv::current()).run_blocking() {
                    eprintln!("Headway could not detach {}: {error}", mount.display());
                }
            });
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
            budget::preflight(file, kind, env, cancel).await?;
            let cmd = Cmd::new("/usr/bin/tar", env).args(["-xf", &file_arg, "-C", &dest_arg]);
            process::run(&cmd, cancel, on_line).await?;
            Ok(unpacked(dest.to_path_buf()))
        }
        ArchiveKind::Zip => {
            budget::preflight(file, kind, env, cancel).await?;
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
            let mut unpacked = Unpacked {
                root: mount.clone(),
                mount: Some(mount),
            };
            if let Err(error) = process::run(&cmd, cancel, on_line).await {
                if let Err(detach_error) = unpacked.detach(env).await {
                    on_line(
                        LogStream::Info,
                        &format!("Could not detach partially attached disk image: {detach_error}"),
                    );
                }
                return Err(error);
            }
            Ok(unpacked)
        }
        ArchiveKind::Raw => {
            let name = file.file_name().ok_or_else(|| {
                Error::InvalidInput(format!("{} has no file name", file.display()))
            })?;
            if !name.to_str().is_some_and(crate::asset::safe_file_name) {
                return Err(Error::NoAsset("raw asset has an unsafe filename".into()));
            }
            let target = dest.join(name);
            let source = file.to_owned();
            tokio::task::spawn_blocking(move || {
                validate_tree(&source)?;
                fs::copy(&source, &target).map_err(|err| Error::io_at("copy", &target, err))?;
                set_executable(&target)
            })
            .await
            .map_err(|error| Error::io("raw copy worker", std::io::Error::other(error)))??;
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
    if let Some(found) = named.iter().find(|path| is_executable(path)) {
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
mod tests;
