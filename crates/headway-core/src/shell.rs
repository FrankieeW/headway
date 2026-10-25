//! The environment that git and build commands run in.
//!
//! Apps launched from Finder get a minimal `PATH` (`/usr/bin:/bin:…`), so the
//! desktop app captures the user's login-shell environment; the CLI simply
//! inherits its terminal's. Either way a few common tool directories are
//! appended so `cargo`, `go`, Homebrew tools … are found.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const MARKER: &str = "__HEADWAY_ENV_BEGIN__";
/// Variables describing the capturing shell itself rather than the user's setup.
const VOLATILE: [&str; 5] = ["PWD", "OLDPWD", "SHLVL", "_", "PS1"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellEnv {
    vars: BTreeMap<String, String>,
}

impl ShellEnv {
    pub fn from_vars(vars: impl IntoIterator<Item = (String, String)>) -> Self {
        let vars = vars
            .into_iter()
            .filter(|(key, _)| !VOLATILE.contains(&key.as_str()))
            .collect();
        Self {
            vars: with_fallback_path(vars),
        }
    }

    /// The current process environment.
    pub fn current() -> Self {
        Self::from_vars(std::env::vars())
    }

    /// The environment of an interactive login shell, falling back to [`ShellEnv::current`].
    pub async fn login_shell() -> Self {
        match capture_login_shell().await {
            Some(vars) => Self::from_vars(vars),
            None => Self::current(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    pub const fn vars(&self) -> &BTreeMap<String, String> {
        &self.vars
    }

    #[must_use]
    pub fn with_var(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let mut vars = self.vars;
        vars.insert(key.into(), value.into());
        Self { vars }
    }

    pub fn path_dirs(&self) -> Vec<PathBuf> {
        self.get("PATH")
            .unwrap_or_default()
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    pub fn contains_dir(&self, dir: &Path) -> bool {
        self.path_dirs().iter().any(|entry| entry == dir)
    }

    /// Resolves `program` against `PATH` like a shell would.
    pub fn which(&self, program: &str) -> Option<PathBuf> {
        if program.contains('/') {
            let path = PathBuf::from(program);
            return is_executable(&path).then_some(path);
        }
        self.path_dirs()
            .into_iter()
            .map(|dir| dir.join(program))
            .find(|candidate| is_executable(candidate))
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

fn fallback_dirs() -> Vec<PathBuf> {
    let home_dirs = [
        ".cargo/bin",
        ".local/bin",
        "go/bin",
        ".bun/bin",
        ".deno/bin",
        ".volta/bin",
    ];
    let system_dirs = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ];
    let home = dirs::home_dir();
    home_dirs
        .iter()
        .filter_map(|dir| home.as_ref().map(|home| home.join(dir)))
        .chain(system_dirs.iter().map(PathBuf::from))
        .collect()
}

fn with_fallback_path(vars: BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut entries: Vec<String> = vars
        .get("PATH")
        .map(|path| {
            path.split(':')
                .filter(|dir| !dir.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for dir in fallback_dirs() {
        let dir_str = dir.display().to_string();
        if dir.is_dir() && !entries.contains(&dir_str) {
            entries.push(dir_str);
        }
    }
    let mut vars = vars;
    vars.insert("PATH".to_owned(), entries.join(":"));
    vars
}

/// Captures the user's interactive login-shell environment.
///
/// The child deliberately inherits the full environment instead of using `process::Cmd`: it is
/// the user's own shell running the user's own rc files, and the captured environment is also
/// Headway's documented token-resolution source (`HEADWAY_GITHUB_TOKEN`, `GITHUB_TOKEN`,
/// `GH_TOKEN`), so stripping those here would silently remove token sources from the desktop
/// app. Any subprocess that executes repository code goes through `process::Cmd`, which strips
/// all three. Execution is bounded (10 s), stdin is closed, and rc output before the marker is
/// ignored so a chatty prompt cannot corrupt the captured environment.
async fn capture_login_shell() -> Option<BTreeMap<String, String>> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| shell.starts_with('/'))
        .unwrap_or_else(|| "/bin/zsh".to_owned());
    let script = format!("printf '%s' '{MARKER}'; /usr/bin/env -0");
    let output = tokio::process::Command::new(shell)
        .args(["-i", "-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(Duration::from_secs(10), output)
        .await
        .ok()?
        .ok()?;
    parse_env_dump(&output.stdout)
}

/// Parses `MARKER` followed by `env -0` output; anything the rc files print before is ignored.
fn parse_env_dump(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.rfind(MARKER)? + MARKER.len();
    let vars: BTreeMap<String, String> = text[start..]
        .split('\0')
        .filter_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            let valid = !key.is_empty() && !key.contains(char::is_whitespace);
            valid.then(|| (key.to_owned(), value.to_owned()))
        })
        .collect();
    (!vars.is_empty()).then_some(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env_dump_after_noise() {
        let dump = b"Welcome to zsh!\n__HEADWAY_ENV_BEGIN__PATH=/usr/bin:/bin\0HOME=/Users/me\0MULTI=a\nb\0";
        let vars = parse_env_dump(dump).unwrap();
        assert_eq!(vars["PATH"], "/usr/bin:/bin");
        assert_eq!(vars["HOME"], "/Users/me");
        assert_eq!(vars["MULTI"], "a\nb");
        assert!(parse_env_dump(b"no marker here").is_none());
    }

    #[test]
    fn drops_volatile_vars_and_keeps_path_order() {
        let env = ShellEnv::from_vars([
            ("PATH".to_owned(), "/custom/bin:/usr/bin".to_owned()),
            ("PWD".to_owned(), "/somewhere".to_owned()),
            ("FOO".to_owned(), "bar".to_owned()),
        ]);
        assert!(env.get("PWD").is_none());
        assert_eq!(env.get("FOO"), Some("bar"));
        let dirs = env.path_dirs();
        assert_eq!(dirs[0], PathBuf::from("/custom/bin"));
        assert_eq!(
            dirs.iter()
                .filter(|dir| *dir == Path::new("/usr/bin"))
                .count(),
            1
        );
        assert!(env.contains_dir(Path::new("/custom/bin")));
    }

    #[test]
    fn which_finds_executables_on_path() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("mytool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain = dir.path().join("notexec");
        std::fs::write(&plain, "").unwrap();

        let env = ShellEnv::from_vars([("PATH".to_owned(), dir.path().display().to_string())]);
        assert_eq!(env.which("mytool"), Some(tool.clone()));
        assert_eq!(env.which(&tool.display().to_string()), Some(tool));
        assert_eq!(env.which("notexec"), None);
        assert_eq!(env.which("definitely-not-a-tool-xyz"), None);
    }

    #[test]
    fn with_var_returns_a_copy() {
        let base = ShellEnv::from_vars([]);
        let next = base.clone().with_var("A", "1");
        assert_eq!(next.get("A"), Some("1"));
        assert_eq!(base.get("A"), None);
    }

    #[tokio::test]
    async fn login_shell_capture_has_a_path() {
        let env = ShellEnv::login_shell().await;
        assert!(env.which("sh").is_some());
    }
}
