//! `headway`: keep apps and CLI tools built from GitHub current on macOS, one
//! commit or one release at a time.
//!
//! Built to be driven by people and by AI agents alike: every run prints exactly
//! one result document on stdout (human text, `--json`, or `--ai` XML), streams
//! progress on stderr, and exits with a stable code (see `headway --help`).

mod args;
mod commands;
mod describe;
mod fields;
mod human;
mod output;
mod progress;
mod xml;

use std::process::ExitCode;

use clap::Parser as _;

use crate::args::Cli;
use crate::output::Printer;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return output::usage_error(&err),
    };
    let printer = Printer::new(&cli);
    let name = cli.command.name();
    match commands::run(cli.command, &printer).await {
        Ok(reply) => printer.success(name, &reply),
        Err(err) => printer.failure(name, &err),
    }
}
