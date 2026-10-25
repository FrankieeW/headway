//! Token, settings, diagnostics and self-description.

use std::fmt::Write as _;
use std::io::{IsTerminal as _, Read as _};

use clap::CommandFactory as _;
use clap_complete::Shell;
use headway_core::config::Settings;
use headway_core::doctor::source_label;
use headway_core::paths::display_path;
use headway_core::{Error, Headway, Result, TokenStatus, token};
use serde_json::{Value, json};

use crate::args::{Cli, SettingsCommand, TokenCommand};
use crate::describe;
use crate::human::{self, Palette};
use crate::output::Reply;

pub async fn token(hw: &Headway, command: TokenCommand, p: Palette) -> Result<Reply> {
    match command {
        TokenCommand::Status => {
            let status = hw.token_status().await?;
            let human = token_human(p, &status);
            let next = if status.masked.is_none()
                || status
                    .error
                    .as_ref()
                    .is_some_and(|error| error.code == "auth_failed")
            {
                vec!["headway token set".to_owned()]
            } else if status.error.is_some() {
                vec!["headway token status".to_owned()]
            } else {
                Vec::new()
            };
            Ok(Reply::new(&status, human).next(next))
        }
        TokenCommand::Set {
            stdin,
            from_gh,
            no_verify,
        } => {
            let secret = if from_gh {
                token::gh_cli_token(&hw.env().await).ok_or_else(|| {
                    Error::InvalidInput(
                        "`gh auth token` returned nothing; run `gh auth login` first".into(),
                    )
                })?
            } else if stdin || !std::io::stdin().is_terminal() {
                let mut input = String::new();
                std::io::stdin()
                    .read_to_string(&mut input)
                    .map_err(|err| Error::io("read the token from stdin", err))?;
                input
            } else {
                rpassword::prompt_password("GitHub token (input hidden): ")
                    .map_err(|err| Error::io("read the token", err))?
            };
            let status = hw.set_token(&secret, !no_verify).await?;
            let who = status
                .login
                .as_deref()
                .map(|login| format!(" for @{login}"))
                .unwrap_or_default();
            let human = format!("{} saved the token{who} to the keychain", p.green("✓"));
            Ok(Reply::new(&status, human))
        }
        TokenCommand::Clear => {
            let removed = hw.clear_token().await?;
            let human = if removed {
                format!("{} removed the token from the keychain", p.green("✓"))
            } else {
                "No token was stored in the keychain.".to_owned()
            };
            Ok(Reply::new(&json!({ "removed": removed }), human))
        }
    }
}

fn token_human(p: Palette, status: &TokenStatus) -> String {
    let mut out = String::new();
    match (&status.masked, &status.login, &status.error) {
        (None, _, _) => {
            let _ = writeln!(
                out,
                "{} no GitHub token (60 requests/hour, public repositories only)",
                p.yellow("!")
            );
        }
        (Some(masked), Some(login), _) => {
            let _ = writeln!(
                out,
                "{} @{login} via {} ({masked})",
                p.green("✓"),
                source_label(status.source)
            );
        }
        (Some(masked), None, error) => {
            let reason = error
                .as_ref()
                .map_or("unverified", |error| error.message.as_str());
            let _ = writeln!(
                out,
                "{} token from {} ({masked}) could not be verified: {reason}",
                p.red("✗"),
                source_label(status.source)
            );
        }
    }
    if let Some(rate) = status.rate {
        let _ = writeln!(
            out,
            "  {} of {} requests left this hour",
            rate.remaining, rate.limit
        );
    }
    out
}

pub fn settings(hw: &Headway, command: SettingsCommand, p: Palette) -> Result<Reply> {
    match command {
        SettingsCommand::Get { key: None } => {
            let settings = hw.settings()?;
            Ok(Reply::new(&settings, settings_human(&settings)))
        }
        SettingsCommand::Get { key: Some(key) } => {
            let settings = serde_json::to_value(hw.settings()?).unwrap_or(Value::Null);
            let value = settings.get(&key).cloned().ok_or_else(|| {
                Error::InvalidInput(format!(
                    "unknown setting `{key}`; expected one of {}",
                    Settings::KEYS.join(", ")
                ))
            })?;
            let human = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            Ok(Reply::new(&json!({ "key": key, "value": value }), human))
        }
        SettingsCommand::Set { key, value } => {
            let settings = hw.set_setting(&key, &value)?;
            let shown = serde_json::to_value(&settings)
                .ok()
                .and_then(|all| all.get(&key).cloned())
                .unwrap_or(Value::Null);
            let human = format!("{} {key} = {shown}", p.green("✓"));
            Ok(Reply::new(&settings, human))
        }
    }
}

fn settings_human(settings: &Settings) -> String {
    let value = serde_json::to_value(settings).unwrap_or(Value::Null);
    Settings::KEYS.iter().fold(String::new(), |mut out, key| {
        let shown = value
            .get(*key)
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
            .unwrap_or_default();
        let _ = writeln!(out, "{key:<24} {shown}");
        out
    })
}

pub async fn doctor(hw: &Headway, p: Palette) -> Reply {
    let report = hw.doctor().await;
    let human = human::doctor(p, &report);
    let exit = u8::from(!report.ok);
    Reply::new(&report, human).exit_code(exit)
}

pub fn paths(hw: &Headway) -> Reply {
    let paths = hw.paths();
    let entries = [
        ("config_file", paths.config_file()),
        ("state_file", paths.state_file()),
        ("cache_dir", paths.cache_dir.clone()),
        ("sources_dir", paths.cache_dir.join("sources")),
        ("log_dir", paths.log_dir.clone()),
    ];
    let human = entries.iter().fold(String::new(), |mut out, (key, path)| {
        let _ = writeln!(out, "{key:<12} {}", display_path(path));
        out
    });
    let data: serde_json::Map<String, Value> = entries
        .iter()
        .map(|(key, path)| ((*key).to_owned(), json!(path.display().to_string())))
        .collect();
    Reply::new(&data, human)
}

pub fn commands() -> Reply {
    let tree = describe::tree();
    let mut human = String::from(
        "headway commands (use --json or --ai for the full machine-readable tree)\n\n",
    );
    let globals = tree["args"]
        .as_array()
        .map(|args| {
            args.iter()
                .filter(|arg| arg["global"] == true)
                .filter_map(|arg| arg["flag"].as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let _ = writeln!(
        human,
        "Global flags (usable after any command): {globals}\n"
    );
    describe_human(&tree, "", &mut human);
    Reply::new(&tree, human)
}

fn describe_human(tree: &Value, prefix: &str, out: &mut String) {
    if let Some(commands) = tree["subcommands"].as_array() {
        for command in commands {
            let path = format!("{prefix}{}", command["name"].as_str().unwrap_or_default());
            let flags = command["args"]
                .as_array()
                .map(|args| {
                    args.iter()
                        .filter(|arg| arg["global"] != true)
                        .map(|arg| {
                            arg["flag"].as_str().map_or_else(
                                || format!("<{}>", arg["name"].as_str().unwrap_or_default()),
                                str::to_owned,
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {path:<24} {}\n  {:<24} {flags}",
                command["about"].as_str().unwrap_or_default(),
                ""
            );
            describe_human(command, &format!("{path} "), out);
        }
    }
}

pub fn completions(shell: Shell) -> Reply {
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "headway", &mut script);
    let script = String::from_utf8_lossy(&script).into_owned();
    Reply::new(
        &json!({ "shell": shell.to_string(), "script": script }),
        script,
    )
}
