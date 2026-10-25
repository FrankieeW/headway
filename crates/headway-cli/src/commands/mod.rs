//! Command implementations. Each returns a [`Reply`]: structured data plus the
//! human rendering and suggested next commands.

mod admin;
mod apps;
mod management;
mod ops;
mod register;
mod revisions;

use headway_core::{AppStatus, AppView, EnvMode, Headway, Result};

use crate::args::Command;
use crate::output::{Printer, Reply};

/// GitHub API base override (GitHub Enterprise, tests).
pub const API_ENV: &str = "HEADWAY_API_URL";

pub async fn run(command: Command, printer: &Printer) -> Result<Reply> {
    let p = printer.palette;
    if matches!(command, Command::Version) {
        return Ok(version());
    }
    let hw = open()?;
    match command {
        Command::Commits {
            id,
            page,
            per_page,
            anchor,
        } => revisions::commits(&hw, &id, page, per_page, anchor.as_deref()).await,
        Command::SourceStorage { id } => revisions::source_storage(&hw, &id),
        Command::CommitOptions { id, reference } => revisions::options(&hw, &id, &reference).await,
        Command::InstallCommit(args) => revisions::install(&hw, &args, printer).await,
        Command::LaunchVariant { id } => revisions::launch(&hw, &id).await,
        Command::Repo(command) => revisions::repositories(&hw, command).await,
        Command::Installation { id } => revisions::installation(&hw, &id),
        Command::Prs { id, page, per_page } => {
            revisions::pull_requests(&hw, &id, page, per_page).await
        }
        Command::InstallPr(mut args) => {
            args.reference = format!(
                "pr:{}",
                args.reference
                    .strip_prefix("pr:")
                    .unwrap_or(&args.reference)
            );
            revisions::install(&hw, &args, printer).await
        }
        Command::Version => Ok(version()),
        Command::List => apps::list(&hw, p),
        Command::Show { id } => apps::show(&hw, &id, p),
        Command::Check { ids } => ops::check(&hw, &ids, p).await,
        Command::Update(args) => ops::update(&hw, &args, printer).await,
        Command::Mark { id, reference } => ops::mark(&hw, &id, &reference, p).await,
        Command::Add(args) => register::add(&hw, &args, p).await,
        Command::Edit(args) => apps::edit(&hw, &args, p),
        Command::Remove { id, purge, dry_run } => apps::remove(&hw, &id, purge, dry_run, p),
        Command::Inspect { repo } => apps::inspect(&hw, &repo, p).await,
        Command::Presets => Ok(apps::presets(p)),
        Command::Discover { sources } => management::discover(&hw, &sources).await,
        Command::Import { keys, dry_run } => management::import(&hw, &keys, dry_run).await,
        Command::Recipes => Ok(management::recipes(&hw)),
        Command::ResumeAutoUpdate { id } => management::resume(&hw, &id),
        Command::AutoUpdate { ids } => management::auto_update(&hw, &ids, printer).await,
        Command::Backups { id } => management::backups(&hw, &id),
        Command::Restore { id, backup_id } => {
            management::restore(&hw, &id, &backup_id, printer).await
        }
        Command::Cli(command) => management::cli(&hw, command).await,
        Command::SelfUpdate(command) => management::self_command(&hw, command).await,
        Command::CommitInfo { id, sha, refresh } => {
            management::commit_info(&hw, &id, &sha, refresh).await
        }
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

fn version() -> Reply {
    let info = Headway::build_info();
    Reply::new(&info, info.to_string()).next(vec!["headway commands --json".to_owned()])
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

/// Candidate configs originate in CLI arguments, not an on-disk config file.
fn input_error(error: headway_core::Error) -> headway_core::Error {
    match error {
        headway_core::Error::Config(message) => headway_core::Error::InvalidInput(message),
        other => other,
    }
}

/// What to run after looking at `view`.
fn next_for(view: &AppView) -> Vec<String> {
    let id = &view.app.id;
    match view.status {
        AppStatus::UpdateAvailable | AppStatus::Diverged => {
            let mut next = vec![format!("headway update {id}")];
            if view.pending_count > 1 && view.app.source.picks_versions() {
                next.push(format!("headway update {id} --next"));
            }
            next
        }
        AppStatus::NotInstalled | AppStatus::Unknown => {
            vec![format!("headway update {id} --dry-run")]
        }
        AppStatus::Unchecked => vec![format!("headway check {id}")],
        AppStatus::Error => vec![format!("headway log {id}"), format!("headway check {id}")],
        AppStatus::Busy | AppStatus::UpToDate => Vec::new(),
    }
}
