//! Parse failures still use the explicitly requested machine format.
use std::process::ExitCode;

use headway_core::ErrorInfo;
use serde_json::json;

use super::{Format, stdout};
use crate::{args::Cli, xml};

pub fn usage_error(err: &clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    let code = u8::try_from(err.exit_code()).unwrap_or(2);
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        return stdout(&err.render().to_string(), code);
    }
    if requested_format() == Format::Human {
        let _ = err.print();
        return ExitCode::from(code);
    }
    let message = err.render().to_string();
    let info = ErrorInfo {
        code: "usage_error".to_owned(),
        exit_code: i32::from(code),
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
    let path = requested_command();
    let command = path.split_whitespace().next().unwrap_or_default();
    let subcommand = path.contains(' ').then_some(path.as_str());
    let text = if requested_format() == Format::Xml {
        xml::document_with_subcommand(command, subcommand, false, &json!({"error": info}), &[])
    } else {
        let mut document = json!({"ok": false, "command": command, "error": info, "next": []});
        if let Some(path) = subcommand {
            document["subcommand"] = json!(path);
        }
        serde_json::to_string_pretty(&document).unwrap_or_default()
    };
    stdout(&text, code)
}

/// Include a recognizable nested path; retain unknown command names for diagnostics.
fn requested_command() -> String {
    let root = <Cli as clap::CommandFactory>::command();
    let mut current = &root;
    let mut path = Vec::new();
    let mut skip_value = false;
    for arg in std::env::args().skip(1) {
        if skip_value {
            skip_value = false;
            continue;
        }
        if arg == "--output" {
            skip_value = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        if let Some(sub) = current.find_subcommand(&arg) {
            path.push(arg);
            current = sub;
        } else if path.is_empty() {
            path.push(arg);
            break;
        } else {
            break;
        }
    }
    path.join(" ")
}

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
