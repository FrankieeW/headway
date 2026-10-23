//! Command implementations. Each returns a [`Reply`]: structured data plus the
//! human rendering and suggested next commands.

mod admin;
mod apps;
mod ops;

use headway_core::{AppStatus, AppView, EnvMode, Headway, Result};

use crate::args::Command;
use crate::output::{Printer, Reply};

/// GitHub API base override (GitHub Enterprise, tests).
pub const API_ENV: &str = "HEADWAY_API_URL";

pub async fn run(command: Command, printer: &Printer) -> Result<Reply> {
    let p = printer.palette;
    let hw = open()?;
    match command {
        Command::List => apps::list(&hw, p),
        Command::Show { id } => apps::show(&hw, &id, p),
        Command::Check { ids } => ops::check(&hw, &ids, p).await,
        Command::Update(args) => ops::update(&hw, &args, printer).await,
        Command::Mark { id, reference } => ops::mark(&hw, &id, &reference, p).await,
        Command::Add(args) => apps::add(&hw, &args, p).await,
        Command::Edit(args) => apps::edit(&hw, &args, p),
        Command::Remove { id, purge, dry_run } => apps::remove(&hw, &id, purge, dry_run, p),
        Command::Inspect { repo } => apps::inspect(&hw, &repo, p).await,
        Command::Presets => Ok(apps::presets(p)),
        Command::History { id, limit } => apps::history(&hw, &id, limit, p),
        Command::Log { id, tail } => apps::log(&hw, &id, tail),
        Command::Token(command) => admin::token(&hw, command, p).await,
        Command::Settings(command) => admin::settings(&hw, command, p),
        Command::Doctor => Ok(admin::doctor(&hw, p).await),
        Command::Paths => Ok(admin::paths(&hw)),
        Command::Commands => Ok(admin::commands()),
        Command::Completions { shell } => Ok(admin::completions(shell)),
    }
}

fn open() -> Result<Headway> {
    let builder = Headway::builder().env_mode(EnvMode::Inherit);
    match std::env::var(API_ENV) {
        Ok(api) if !api.trim().is_empty() => builder.api(api.trim()).build(),
        _ => builder.build(),
    }
}

/// The current command line without `--dry-run`: what to run to do it for real.
fn without_dry_run() -> String {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| arg != "--dry-run")
        .map(|arg| headway_core::process::shell_quote(&arg))
        .collect();
    format!("headway {}", args.join(" "))
}

/// What to run after looking at `view`.
fn next_for(view: &AppView) -> Vec<String> {
    let id = &view.app.id;
    match view.status {
        AppStatus::UpdateAvailable | AppStatus::Diverged => {
            let mut next = vec![format!("headway update {id}")];
            if view.pending_count > 1 {
                next.push(format!("headway update {id} --next"));
            }
            next
        }
        AppStatus::NotInstalled => vec![
            format!("headway update {id}"),
            format!("headway mark {id} latest"),
        ],
        AppStatus::Unchecked => vec![format!("headway check {id}")],
        AppStatus::Error => vec![format!("headway log {id}"), format!("headway check {id}")],
        AppStatus::Busy | AppStatus::UpToDate => Vec::new(),
    }
}
