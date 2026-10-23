//! `headway doctor`: everything that commonly stops an update, checked up front.

use serde::Serialize;

use crate::config::{Install, Settings, TrackedApp};
use crate::error::Error;
use crate::paths::{display_path, expand_tilde};
use crate::process::{self, Cmd};
use crate::service::TokenStatus;
use crate::shell::ShellEnv;
use crate::store::Store;
use crate::token::TokenSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// No check failed (warnings are fine).
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn new(checks: Vec<DoctorCheck>) -> Self {
        Self {
            ok: checks
                .iter()
                .all(|check| check.status != CheckStatus::Error),
            checks,
        }
    }
}

fn check(
    name: &str,
    status: CheckStatus,
    message: impl Into<String>,
    hint: Option<&str>,
) -> DoctorCheck {
    DoctorCheck {
        name: name.to_owned(),
        status,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

pub fn error_check(name: &str, err: &Error) -> DoctorCheck {
    DoctorCheck {
        name: name.to_owned(),
        status: CheckStatus::Error,
        message: err.to_string(),
        hint: err.hint(),
    }
}

pub fn config_check(store: &Store) -> DoctorCheck {
    match (store.config(), store.state()) {
        (Ok(config), Ok(_)) => check(
            "config",
            CheckStatus::Ok,
            format!(
                "{} app(s) tracked in {}",
                config.apps.len(),
                display_path(&store.paths().config_file())
            ),
            None,
        ),
        (Err(err), _) | (_, Err(err)) => error_check("config", &err),
    }
}

pub async fn git_check(env: &ShellEnv) -> DoctorCheck {
    let version = process::capture(&Cmd::new("git", env).arg("--version")).await;
    match version {
        Ok(out) if out.success() => check("git", CheckStatus::Ok, out.stdout.trim(), None),
        _ => check(
            "git",
            CheckStatus::Error,
            "git is not available",
            Some("install the Command Line Tools with `xcode-select --install`"),
        ),
    }
}

pub fn token_check(status: &TokenStatus) -> DoctorCheck {
    let rate = status
        .rate
        .map(|rate| format!(" ({}/{} requests left)", rate.remaining, rate.limit))
        .unwrap_or_default();
    if let Some(err) = &status.error {
        return DoctorCheck {
            name: "github".to_owned(),
            status: CheckStatus::Error,
            message: format!(
                "the {} token was rejected: {}",
                source_label(status.source),
                err.message
            ),
            hint: Some("set a new token with `headway token set`".to_owned()),
        };
    }
    match &status.login {
        Some(login) => check(
            "github",
            CheckStatus::Ok,
            format!(
                "authenticated as @{login} via {}{rate}",
                source_label(status.source)
            ),
            None,
        ),
        None => check(
            "github",
            CheckStatus::Warn,
            format!("no GitHub token: 60 requests/hour and no private repositories{rate}"),
            Some("`headway token set` (or `gh auth login`)"),
        ),
    }
}

pub const fn source_label(source: TokenSource) -> &'static str {
    match source {
        TokenSource::HeadwayEnv => "HEADWAY_GITHUB_TOKEN",
        TokenSource::Keychain => "the keychain",
        TokenSource::GithubEnv => "GITHUB_TOKEN",
        TokenSource::GhEnv => "GH_TOKEN",
        TokenSource::GhCli => "gh auth token",
        TokenSource::None => "nothing",
    }
}

pub fn bin_dir_check(settings: &Settings, env: &ShellEnv) -> DoctorCheck {
    let dir = expand_tilde(&settings.bin_dir);
    if env.contains_dir(&dir) {
        check(
            "bin_dir",
            CheckStatus::Ok,
            format!("{} is on PATH", settings.bin_dir),
            None,
        )
    } else {
        DoctorCheck {
            name: "bin_dir".to_owned(),
            status: CheckStatus::Warn,
            message: format!(
                "{} is not on PATH, so installed tools will not be found",
                settings.bin_dir
            ),
            hint: Some(format!(
                "add `export PATH=\"{}:$PATH\"` to ~/.zprofile",
                display_path(&dir).replace('~', "$HOME")
            )),
        }
    }
}

pub fn apps_dir_check(settings: &Settings) -> DoctorCheck {
    let dir = expand_tilde(&settings.apps_dir);
    let writable = tempfile::tempfile_in(&dir).is_ok();
    if writable {
        check(
            "apps_dir",
            CheckStatus::Ok,
            format!("{} is writable", settings.apps_dir),
            None,
        )
    } else {
        check(
            "apps_dir",
            CheckStatus::Warn,
            format!(
                "cannot write to {}; app installs will fail",
                settings.apps_dir
            ),
            Some("use ~/Applications with `headway settings set apps_dir ~/Applications`"),
        )
    }
}

/// Programs a source build needs that are not on PATH.
pub fn tools_check(app: &TrackedApp, env: &ShellEnv) -> Option<DoctorCheck> {
    let Install::Source(source) = &app.install else {
        return None;
    };
    let missing: Vec<String> = command_programs(&source.build)
        .into_iter()
        .filter(|program| env.which(program).is_none())
        .collect();
    let name = format!("app:{}", app.id);
    Some(if missing.is_empty() {
        check(&name, CheckStatus::Ok, "build tools found", None)
    } else {
        DoctorCheck {
            name,
            status: CheckStatus::Warn,
            message: format!("not on PATH: {}", missing.join(", ")),
            hint: Some(format!(
                "install them, or change the build with `headway edit {} --build '…'`",
                app.id
            )),
        }
    })
}

const SHELL_WORDS: [&str; 17] = [
    "cd", "export", "set", "echo", "test", "[", "true", "false", "exec", "source", ".", "if",
    "then", "fi", "for", "do", "done",
];

/// The programs a shell command line runs (first word of each simple command).
pub fn command_programs(command: &str) -> Vec<String> {
    let mut programs: Vec<String> = Vec::new();
    for segment in command.split(['&', '|', ';', '\n', '(', ')']) {
        let program = segment.split_whitespace().find(|word| !is_assignment(word));
        if let Some(program) = program {
            let program = program.trim_matches(['"', '\'']);
            if !program.is_empty()
                && !SHELL_WORDS.contains(&program)
                && !programs.iter().any(|p| p == program)
            {
                programs.push(program.to_owned());
            }
        }
    }
    programs
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::cli_app;

    #[test]
    fn extracts_programs_from_build_commands() {
        assert_eq!(
            command_programs("cargo build --release --locked"),
            ["cargo"]
        );
        assert_eq!(
            command_programs("pnpm install && CI=1 pnpm tauri build; make -C x | tee log"),
            ["pnpm", "make", "tee"]
        );
        assert_eq!(command_programs("cd sub && ./build.sh"), ["./build.sh"]);
        assert!(command_programs("").is_empty());
    }

    #[test]
    fn missing_tools_are_warnings() {
        let env = ShellEnv::from_vars([("PATH".to_owned(), "/nonexistent".to_owned())]);
        let app = cli_app("rg");
        let found = tools_check(&app, &env).unwrap();
        assert!(found.message.contains("cargo") || found.status == CheckStatus::Ok);
        let report = DoctorReport::new(vec![found]);
        assert!(report.ok, "warnings do not fail the report");
    }

    #[test]
    fn bin_dir_on_path() {
        let settings = Settings {
            bin_dir: "/opt/tools/bin".into(),
            ..Settings::default()
        };
        let on = ShellEnv::from_vars([("PATH".to_owned(), "/opt/tools/bin".to_owned())]);
        assert_eq!(bin_dir_check(&settings, &on).status, CheckStatus::Ok);
        let off = ShellEnv::from_vars([("PATH".to_owned(), "/usr/bin".to_owned())]);
        let warn = bin_dir_check(&settings, &off);
        assert_eq!(warn.status, CheckStatus::Warn);
        assert!(warn.hint.unwrap().contains("/opt/tools/bin"));
    }
}
