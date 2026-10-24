//! Checking, updating and marking.

use std::fmt::Write as _;

use headway_core::update::short;
use headway_core::{AppView, CancellationToken, Error, ErrorInfo, Headway, Result, Target};
use serde_json::{Value, json};

use super::{next_for, without_dry_run};
use crate::args::UpdateArgs;
use crate::human::{self, Palette};
use crate::output::{Printer, Reply};
use crate::progress::TerminalSink;

pub async fn check(hw: &Headway, ids: &[String], p: Palette) -> Result<Reply> {
    let results = hw.check_many(ids).await?;
    let mut human = String::new();
    let mut records = Vec::new();
    let mut behind = Vec::new();
    let mut first_failure: Option<i32> = None;
    let width = results.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
    for (id, result) in &results {
        match result {
            Ok(view) => {
                let (status, style) = human::status_cell(view);
                let latest = view
                    .last_check
                    .as_ref()
                    .and_then(|check| check.latest.as_ref())
                    .filter(|_| view.has_update())
                    .map(|latest| format!(" · latest {} {}", short(&latest.id), latest.title))
                    .unwrap_or_default();
                let _ = writeln!(
                    human,
                    "{id:<width$}  {}{}",
                    style.apply(p, &status),
                    p.dim(&latest)
                );
                if view.has_update() {
                    behind.push(id.clone());
                }
                records.push(json!({
                    "id": id,
                    "ok": true,
                    "status": view.status,
                    "pending_count": view.pending_count,
                    "latest": view.last_check.as_ref().and_then(|check| check.latest.as_ref()).map(|latest| json!({
                        "id": latest.id, "title": latest.title, "date": latest.date,
                    })),
                }));
            }
            Err(err) => {
                first_failure.get_or_insert(err.exit_code());
                let _ = writeln!(human, "{id:<width$}  {} {err}", p.red("✗"));
                records.push(json!({"id": id, "ok": false, "error": ErrorInfo::from(err)}));
            }
        }
    }
    if results.is_empty() {
        human.push_str("No apps tracked yet. Start with `headway add owner/repo`.");
    }
    let data = json!({
        "results": records,
        "checked": results.len(),
        "with_updates": behind.len(),
    });
    let next: Vec<String> = results
        .iter()
        .filter_map(|(_, result)| result.as_ref().ok())
        .flat_map(next_for)
        .take(6)
        .collect();
    let exit = first_failure.map_or(0, |code| u8::try_from(code).unwrap_or(1));
    Ok(Reply::new(&data, human).next(next).exit_code(exit))
}

fn target(args: &UpdateArgs) -> Target {
    match (&args.to, args.next) {
        (Some(reference), _) => Target::Ref(reference.clone()),
        (None, true) => Target::Next,
        (None, false) => Target::Latest,
    }
}

pub async fn update(hw: &Headway, args: &UpdateArgs, printer: &Printer) -> Result<Reply> {
    let p = printer.palette;
    let target = target(args);
    let ids: Vec<String> = if args.all {
        hw.check_many(&[])
            .await?
            .into_iter()
            .filter_map(|(id, result)| result.ok().filter(AppView::has_update).map(|_| id))
            .collect()
    } else if args.ids.is_empty() {
        return Err(Error::InvalidInput(
            "name the apps to update, or pass --all".into(),
        ));
    } else {
        args.ids.clone()
    };
    if ids.is_empty() {
        return Ok(Reply::new(
            &json!({"results": [], "updated": 0}),
            "Everything is up to date.",
        ));
    }

    if args.dry_run {
        let mut plans = Vec::new();
        let mut human = String::new();
        for id in &ids {
            let plan = hw.plan(id, &target).await?;
            human.push_str(&human::plan(p, &plan));
            plans.push(plan);
        }
        return Ok(Reply::new(&json!({ "plans": plans }), human).next(vec![without_dry_run()]));
    }

    let sink = TerminalSink::new(printer.log_level(), p.color);
    let cancel = CancellationToken::new();
    let watcher = {
        let cancel = cancel.clone();
        // Ctrl-C, SIGTERM (an agent's timeout) or SIGHUP stop the running build — its whole
        // process group — instead of orphaning it with the lock released; exit code 7.
        tokio::spawn(async move {
            interrupted().await;
            cancel.cancel();
        })
    };
    let mut outcomes = Vec::new();
    for id in &ids {
        if cancel.is_cancelled() {
            outcomes.push((id.clone(), Err(Error::Cancelled)));
            continue;
        }
        outcomes.push((id.clone(), hw.update(id, &target, &sink, &cancel).await));
    }
    watcher.abort();

    // A single app reports its failure as the command's error.
    if let [(_, Err(_))] = outcomes.as_slice()
        && let Some((_, Err(err))) = outcomes.pop()
    {
        return Err(err);
    }
    let mut human = String::new();
    let mut records = Vec::new();
    let mut first_failure: Option<i32> = None;
    for (id, outcome) in &outcomes {
        match outcome {
            Ok(outcome) => {
                let _ = writeln!(human, "{}", human::outcome(p, outcome));
                let mut record = serde_json::to_value(outcome).unwrap_or(Value::Null);
                if let Value::Object(map) = &mut record {
                    map.insert("ok".to_owned(), Value::Bool(true));
                }
                records.push(record);
            }
            Err(err) => {
                first_failure.get_or_insert(err.exit_code());
                let _ = writeln!(human, "{} {} {err}", p.red("✗"), p.bold(id));
                records.push(json!({"app_id": id, "ok": false, "error": ErrorInfo::from(err)}));
            }
        }
    }
    let updated = outcomes
        .iter()
        .filter(|(_, outcome)| outcome.as_ref().is_ok_and(|outcome| outcome.changed))
        .count();
    let failed: Vec<String> = outcomes
        .iter()
        .filter(|(_, outcome)| outcome.is_err())
        .map(|(id, _)| format!("headway log {id}"))
        .collect();
    let data = json!({ "results": records, "updated": updated });
    let exit = first_failure.map_or(0, |code| u8::try_from(code).unwrap_or(1));
    Ok(Reply::new(&data, human).next(failed).exit_code(exit))
}

/// Resolves on Ctrl-C, SIGTERM or SIGHUP.
pub(super) async fn interrupted() {
    use tokio::signal::unix::{SignalKind, signal};
    let on = |kind: SignalKind| async move {
        match signal(kind) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = on(SignalKind::terminate()) => {}
        () = on(SignalKind::hangup()) => {}
    }
}

pub async fn mark(hw: &Headway, id: &str, reference: &str, p: Palette) -> Result<Reply> {
    let view = hw.mark(id, reference).await?;
    let installed = view
        .installed
        .as_ref()
        .map(|installed| {
            installed
                .version
                .clone()
                .unwrap_or_else(|| short(&installed.id))
        })
        .unwrap_or_default();
    let (status, style) = human::status_cell(&view);
    let human = format!(
        "{} marked `{id}` as installed at {installed}  {}",
        p.green("✓"),
        style.apply(p, &status)
    );
    let next = next_for(&view);
    Ok(Reply::new(&view, human).next(next))
}
