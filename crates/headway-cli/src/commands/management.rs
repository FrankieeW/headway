use std::fmt::Write as _;

use headway_core::{CancellationToken, Error, Headway, Result, TrackedApp};
use serde_json::json;

use crate::management_args::{CliCommand, SelfCommand};
use crate::output::{Printer, Reply};
use crate::progress::TerminalSink;

pub async fn discover(hw: &Headway, sources: &[String]) -> Result<Reply> {
    let report = hw.discover_apps(sources).await?;
    let mut human = String::new();
    for candidate in &report.candidates {
        let _ = writeln!(
            human,
            "{}  {}  {}",
            candidate.key,
            candidate.installed_version.as_deref().unwrap_or("unknown"),
            candidate.conflicts.join("; ")
        );
        if let headway_core::Source::Brew(source) = &candidate.app.source
            && let Some(path) = &source.install_path
        {
            let _ = writeln!(human, "  {path}");
        }
    }
    for issue in &report.errors {
        let _ = writeln!(human, "{}: {}", issue.source, issue.error.message);
    }
    let next = report
        .candidates
        .iter()
        .filter(|item| !item.already_tracked)
        .take(3)
        .map(|item| {
            format!(
                "headway import {} --dry-run",
                headway_core::process::shell_quote(&item.key)
            )
        })
        .collect();
    let exit = u8::from(!report.errors.is_empty());
    Ok(Reply::new(&report, human).next(next).exit_code(exit))
}

pub async fn import(hw: &Headway, keys: &[String], dry_run: bool) -> Result<Reply> {
    let needs_inventory = keys.iter().any(|key| !key.starts_with("recipe:"));
    let report = if needs_inventory {
        hw.discover_apps(&[]).await?
    } else {
        headway_core::InventoryReport::default()
    };
    let recipes = hw.recipes();
    let mut apps: Vec<TrackedApp> = Vec::new();
    for key in keys {
        let app = match key.strip_prefix("recipe:") {
            Some(id) => recipes
                .iter()
                .find(|recipe| recipe.id == id)
                .map(|recipe| recipe.app.clone()),
            None => report
                .candidates
                .iter()
                .find(|candidate| candidate.key == *key)
                .map(|candidate| candidate.app.clone()),
        }
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "`{key}` is not available; run `headway discover` or `headway recipes`"
            ))
        })?;
        app.validate()?;
        apps.push(app);
    }
    let apps = hw.plan_import_apps(apps)?;
    let ids: Vec<String> = apps.iter().map(|app| app.id.clone()).collect();
    if dry_run {
        return Ok(Reply::new(
            &json!({"apps": apps, "saved": false}),
            format!("Would track: {}", ids.join(", ")),
        )
        .next(vec![super::without_dry_run()]));
    }
    let views = hw.import_apps(apps)?;
    Ok(Reply::new(
        &json!({"apps":views,"saved":true}),
        format!("Tracking: {}", ids.join(", ")),
    )
    .next(vec![format!("headway check {}", ids.join(" "))]))
}

pub fn recipes(hw: &Headway) -> Reply {
    let recipes = hw.recipes();
    let human = recipes
        .iter()
        .map(|recipe| format!("{}  {}", recipe.id, recipe.description))
        .collect::<Vec<_>>()
        .join("\n");
    Reply::new(&recipes, human).next(vec!["headway import recipe:headway-app --dry-run".into()])
}

pub async fn commit_info(hw: &Headway, id: &str, sha: &str, refresh: bool) -> Result<Reply> {
    let metadata = hw.commit_metadata(id, sha, refresh).await?;
    let human = format!(
        "{}  version {}  CI {:?}  release {:?}",
        metadata.sha,
        metadata.version.as_deref().unwrap_or("unknown"),
        metadata.ci_state,
        metadata.is_release
    );
    Ok(Reply::new(&metadata, human).next(vec![format!("headway show {id}")]))
}

pub fn resume(hw: &Headway, id: &str) -> Result<Reply> {
    let view = hw.resume_auto_update(id)?;
    Ok(
        Reply::new(&view, format!("Automatic updates resumed for {id}"))
            .next(vec![format!("headway check {id}")]),
    )
}

pub async fn auto_update(hw: &Headway, ids: &[String], printer: &Printer) -> Result<Reply> {
    let ids = if ids.is_empty() {
        hw.list()?.into_iter().map(|view| view.app.id).collect()
    } else {
        ids.to_vec()
    };
    let cancel = CancellationToken::new();
    let signal_cancel = cancel.clone();
    let watcher = tokio::spawn(async move {
        super::ops::interrupted().await;
        signal_cancel.cancel();
    });
    let sink = TerminalSink::new(printer.log_level(), printer.palette.color);
    let mut records = Vec::new();
    let mut updated_steps = 0;
    let mut exit = 0;
    for id in &ids {
        match hw.auto_update(id, &sink, &cancel).await {
            Ok(outcomes) => {
                updated_steps += outcomes.iter().filter(|outcome| outcome.changed).count();
                records.push(json!({"id":id,"ok":true,"outcomes":outcomes}));
            }
            Err(error) => {
                if exit == 0 {
                    exit = u8::try_from(error.exit_code()).unwrap_or(1);
                }
                records.push(
                    json!({"id":id,"ok":false,"error":headway_core::ErrorInfo::from(&error)}),
                );
            }
        }
    }
    watcher.abort();
    Ok(Reply::new(
        &json!({"results":records,"updated_steps":updated_steps}),
        format!(
            "Automatic cycle: {} subscriptions, {updated_steps} update steps",
            ids.len()
        ),
    )
    .exit_code(exit))
}

pub fn backups(hw: &Headway, id: &str) -> Result<Reply> {
    let backups = hw.list_backups(id)?;
    let human = backups
        .iter()
        .map(|backup| {
            format!(
                "{}  {}",
                backup.id,
                backup
                    .installed
                    .as_ref()
                    .map_or("unknown", |installed| installed.id.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let next = backups
        .first()
        .map(|backup| vec![format!("headway restore {id} {}", backup.id)])
        .unwrap_or_default();
    Ok(Reply::new(&backups, human).next(next))
}

pub async fn restore(hw: &Headway, id: &str, backup_id: &str, printer: &Printer) -> Result<Reply> {
    let cancel = CancellationToken::new();
    let signal_cancel = cancel.clone();
    let watcher = tokio::spawn(async move {
        super::ops::interrupted().await;
        signal_cancel.cancel();
    });
    let sink = TerminalSink::new(printer.log_level(), printer.palette.color);
    let outcome = hw.restore_backup(id, backup_id, &sink, &cancel).await;
    watcher.abort();
    let outcome = outcome?;
    Ok(
        Reply::new(&outcome, format!("Restored {id} from {backup_id}"))
            .next(vec![format!("headway show {id}")]),
    )
}

pub async fn cli(hw: &Headway, command: CliCommand) -> Result<Reply> {
    let status = match command {
        CliCommand::Status { app } => hw.cli_status(app.as_deref()).await?,
        CliCommand::Fix { app } => hw.fix_cli_link(app.as_deref()).await?,
    };
    let human = format!(
        "{} → {}{}",
        status.link_path,
        status.points_to.as_deref().unwrap_or("not linked"),
        if status.shadowed {
            " (another headway precedes this link on PATH)"
        } else {
            ""
        }
    );
    let next = if status.needs_repair {
        vec!["headway cli fix".into()]
    } else {
        Vec::new()
    };
    Ok(Reply::new(&status, human).next(next))
}

pub async fn self_command(hw: &Headway, command: SelfCommand) -> Result<Reply> {
    if matches!(command, SelfCommand::Subscribe) {
        let view = hw.ensure_self_subscription().await?;
        let message = if view
            .app
            .github()
            .is_some_and(|github| github.options.variant.is_some())
        {
            "Independent Headway keeps the shared subscriptions"
        } else {
            "Headway is the first subscription"
        };
        return Ok(Reply::new(&view, message).next(vec![format!("headway show {}", view.app.id)]));
    }
    let status = match command {
        SelfCommand::Status => hw.self_update_status().await?,
        SelfCommand::Restart => hw.request_self_restart().await?,
        SelfCommand::Subscribe => return Err(Error::InvalidInput("invalid self command".into())),
    };
    let next = if (status.pending_restart || status.restart_error.is_some())
        && !status.restart_requested
    {
        vec!["headway self restart".into()]
    } else {
        Vec::new()
    };
    Ok(Reply::new(
        &status,
        format!(
            "Running {}; installed {}",
            status.running,
            status
                .installed
                .as_ref()
                .map_or_else(|| "unknown".into(), ToString::to_string)
        ),
    )
    .next(next))
}
