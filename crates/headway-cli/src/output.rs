//! Printing results: exactly one document on stdout per run, in the chosen
//! format; progress and logs go to stderr.

use std::io::IsTerminal as _;
use std::process::ExitCode;

use clap::ValueEnum;
use headway_core::{Error, ErrorInfo};
use serde::Serialize;
use serde_json::{Value, json};

use crate::args::Cli;
use crate::human::Palette;
use crate::progress::LogLevel;
use crate::xml;

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
    quiet: bool,
    verbose: bool,
}

impl Printer {
    pub fn new(cli: &Cli) -> Self {
        let color = !cli.no_color
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal();
        Self {
            format: cli.format(),
            palette: Palette { color },
            quiet: cli.quiet,
            verbose: cli.verbose,
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
        match self.format {
            Format::Human => {
                let text = reply.human.trim_end();
                if !text.is_empty() {
                    println!("{text}");
                }
                if !reply.next.is_empty() && !self.quiet {
                    println!(
                        "{}",
                        self.palette
                            .dim(&format!("next: {}", reply.next.join("  |  ")))
                    );
                }
            }
            Format::Json => {
                let document =
                    json!({"ok": true, "command": command, "data": reply.data, "next": reply.next});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&document).unwrap_or_default()
                );
            }
            Format::Xml => println!("{}", xml::document(command, true, &reply.data, &reply.next)),
        }
        ExitCode::from(reply.exit_code)
    }

    pub fn failure(&self, command: &str, err: &Error) -> ExitCode {
        let exit_code = u8::try_from(err.exit_code()).unwrap_or(1);
        let info = ErrorInfo::from(err);
        match self.format {
            Format::Human => {
                let p = Palette {
                    color: self.palette.color && std::io::stderr().is_terminal(),
                };
                eprintln!("{} {}", p.red("error:"), info.message);
                let skip = info.details.len().saturating_sub(15);
                for line in &info.details[skip..] {
                    eprintln!("  {}", p.dim(line));
                }
                if let Some(hint) = &info.hint {
                    eprintln!("{} {hint}", p.yellow("hint:"));
                }
            }
            Format::Json => {
                let document = json!({"ok": false, "command": command, "error": error_value(&info, exit_code)});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&document).unwrap_or_default()
                );
            }
            Format::Xml => {
                let data = json!({"error": error_value(&info, exit_code)});
                println!("{}", xml::document(command, false, &data, &[]));
            }
        }
        ExitCode::from(exit_code)
    }
}

fn error_value(info: &ErrorInfo, exit_code: u8) -> Value {
    let mut value = serde_json::to_value(info).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        map.insert("exit_code".to_owned(), json!(exit_code));
    }
    value
}

/// Reports a command line clap could not parse, structured when JSON/XML was asked for.
pub fn usage_error(err: &clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    let code = u8::try_from(err.exit_code()).unwrap_or(2);
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) || requested_format() == Format::Human
    {
        let _ = err.print();
        return ExitCode::from(code);
    }
    let message = err.render().to_string();
    let info = ErrorInfo {
        code: "usage_error".to_owned(),
        message: message
            .lines()
            .next()
            .unwrap_or_default()
            .trim_start_matches("error: ")
            .to_owned(),
        hint: Some(
            "run `headway commands --json` (or `headway <command> --help`) for the accepted flags"
                .to_owned(),
        ),
        details: message
            .lines()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect(),
    };
    let value = error_value(&info, code);
    let command = requested_command();
    match requested_format() {
        Format::Xml => println!(
            "{}",
            xml::document(&command, false, &json!({"error": value}), &[])
        ),
        _ => println!(
            "{}",
            serde_json::to_string_pretty(&json!({"ok": false, "command": command, "error": value}))
                .unwrap_or_default()
        ),
    }
    ExitCode::from(code)
}

/// The subcommand named on a command line that failed to parse, if any.
fn requested_command() -> String {
    let command = <Cli as clap::CommandFactory>::command();
    let names: Vec<&str> = command
        .get_subcommands()
        .map(clap::Command::get_name)
        .collect();
    std::env::args()
        .skip(1)
        .find(|arg| names.contains(&arg.as_str()))
        .unwrap_or_default()
}

/// The format asked for on a command line that failed to parse.
fn requested_format() -> Format {
    let args: Vec<String> = std::env::args().collect();
    let flag_value = args
        .iter()
        .position(|arg| arg == "--output")
        .and_then(|index| args.get(index + 1).cloned())
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix("--output=").map(str::to_owned))
        })
        .or_else(|| std::env::var("HEADWAY_OUTPUT").ok());
    if args.iter().any(|arg| arg == "--ai") {
        Format::Xml
    } else if args.iter().any(|arg| arg == "--json") {
        Format::Json
    } else {
        match flag_value.as_deref() {
            Some("json") => Format::Json,
            Some("xml") => Format::Xml,
            _ => Format::Human,
        }
    }
}
