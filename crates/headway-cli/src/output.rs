//! One result document on stdout; progress and logs go to stderr.
//! Help/version deliberately retain clap's conventional human output.

use std::io::{IsTerminal as _, Write as _};
use std::process::ExitCode;

use clap::ValueEnum;
use headway_core::{Error, ErrorInfo};
use serde::Serialize;
use serde_json::{Value, json};

use crate::args::Cli;
use crate::human::Palette;
use crate::progress::LogLevel;
use crate::xml;

#[path = "output_usage.rs"]
mod usage;
pub use usage::usage_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Text and tables for people
    Human,
    /// `{"ok", "command", "data", "next"}`
    Json,
    /// Compact XML for AI agents (`--ai`)
    Xml,
}

/// What a command produced.
pub struct Reply {
    pub data: Value,
    pub human: String,
    /// Suggested follow-up commands.
    pub next: Vec<String>,
    /// Non-zero when the command ran but part of its work failed.
    pub exit_code: u8,
}

impl Reply {
    pub fn new(data: &impl Serialize, human: impl Into<String>) -> Self {
        Self {
            data: serde_json::to_value(data).unwrap_or(Value::Null),
            human: human.into(),
            next: Vec::new(),
            exit_code: 0,
        }
    }

    #[must_use]
    pub fn next(self, next: Vec<String>) -> Self {
        Self { next, ..self }
    }

    #[must_use]
    pub fn exit_code(self, exit_code: u8) -> Self {
        Self { exit_code, ..self }
    }
}

pub struct Printer {
    pub format: Format,
    pub palette: Palette,
    pub stderr_color: bool,
    quiet: bool,
    verbose: bool,
    subcommand: Option<&'static str>,
}

impl Printer {
    pub fn new(cli: &Cli) -> Self {
        Self {
            format: cli.format(),
            palette: Palette {
                color: color_enabled(cli.no_color, std::io::stdout().is_terminal()),
            },
            stderr_color: color_enabled(cli.no_color, std::io::stderr().is_terminal()),
            quiet: cli.quiet,
            verbose: cli.verbose,
            subcommand: cli.command.subcommand(),
        }
    }

    /// How much of an update's output goes to stderr.
    pub fn log_level(&self) -> LogLevel {
        if self.quiet {
            LogLevel::Silent
        } else if self.verbose || self.format == Format::Human {
            LogLevel::Full
        } else {
            LogLevel::Stages
        }
    }

    pub fn success(&self, command: &str, reply: &Reply) -> ExitCode {
        let next: Vec<_> = reply
            .next
            .iter()
            .map(|hint| format_hint(self.format, hint))
            .collect();
        let text = match self.format {
            Format::Human => {
                let mut text = reply.human.trim_end().to_owned();
                if !next.is_empty() && !self.quiet {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&self.palette.dim(&format!("next: {}", next.join("  |  "))));
                }
                text
            }
            Format::Json => {
                let mut document = json!({"ok": reply.exit_code == 0, "command": command,
                    "data": reply.data, "next": next});
                if let Some(path) = self.subcommand {
                    document["subcommand"] = json!(path);
                }
                serde_json::to_string_pretty(&document).unwrap_or_default()
            }
            Format::Xml => xml::document_with_subcommand(
                command,
                self.subcommand,
                reply.exit_code == 0,
                &reply.data,
                &next,
            ),
        };
        stdout(&text, reply.exit_code)
    }

    pub fn failure(&self, command: &str, err: &Error) -> ExitCode {
        let exit_code = u8::try_from(err.exit_code()).unwrap_or(1);
        let info = ErrorInfo::from(err);
        match self.format {
            Format::Human => {
                let p = Palette {
                    color: self.stderr_color,
                };
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "{} {}",
                    p.red("error:"),
                    info.message
                );
                let skip = info.details.len().saturating_sub(15);
                for line in &info.details[skip..] {
                    let _ = writeln!(std::io::stderr().lock(), "  {}", p.dim(line));
                }
                if let Some(hint) = &info.hint {
                    let _ = writeln!(std::io::stderr().lock(), "{} {hint}", p.yellow("hint:"));
                }
                ExitCode::from(exit_code)
            }
            Format::Json => {
                let mut document =
                    json!({"ok": false, "command": command, "error": info, "next": []});
                if let Some(path) = self.subcommand {
                    document["subcommand"] = json!(path);
                }
                stdout(
                    &serde_json::to_string_pretty(&document).unwrap_or_default(),
                    exit_code,
                )
            }
            Format::Xml => stdout(
                &xml::document_with_subcommand(
                    command,
                    self.subcommand,
                    false,
                    &json!({"error": info}),
                    &[],
                ),
                exit_code,
            ),
        }
    }
}

/// Explicit disables win over force-color; stdout and stderr have separate TTY policy.
fn color_enabled(disabled: bool, tty: bool) -> bool {
    !disabled
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").as_deref() != Ok("dumb")
        && std::env::var("CLICOLOR").as_deref() != Ok("0")
        && (tty
            || std::env::var("CLICOLOR_FORCE").is_ok_and(|value| !value.is_empty() && value != "0"))
}

fn format_hint(format: Format, hint: &str) -> String {
    let flag = match format {
        Format::Human => return hint.to_owned(),
        Format::Json => "--json",
        Format::Xml => "--ai",
    };
    if hint
        .split_whitespace()
        .any(|part| part == flag || part == format!("'{flag}'"))
    {
        return hint.to_owned();
    }
    hint.strip_prefix("headway ")
        .map_or_else(|| hint.to_owned(), |tail| format!("headway {flag} {tail}"))
}

/// A closed downstream pipe is normal consumption, not a panic or an error document.
/// Other write errors are failures and must not be silently reported as success.
fn stdout(text: &str, code: u8) -> ExitCode {
    if text.is_empty() {
        return ExitCode::from(code);
    }
    let newline = if text.ends_with('\n') { "" } else { "\n" };
    match write!(std::io::stdout().lock(), "{text}{newline}") {
        Ok(()) => ExitCode::from(code),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(std::io::stderr().lock(), "error: write stdout: {error}");
            ExitCode::FAILURE
        }
    }
}
