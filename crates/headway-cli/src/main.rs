//! `headway`: keep apps and CLI tools built from GitHub current on macOS, one
//! commit or one release at a time.
//!
//! Result documents use human text, `--json`, or `--ai` XML; progress is on stderr.
//! Help/version retain conventional text. Signals can stop read-only commands
//! before a result; closed downstream pipes are handled quietly.

mod args;
mod args_fields;
mod args_paths;
mod commands;
mod describe;
mod fields;
mod human;
mod management_args;
mod output;
mod progress;
mod revision_args;
mod source_fields;
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
