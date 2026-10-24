//! Managing tracked apps: list, show, add, edit, remove, inspect, history, log.

use headway_core::config::{Track, TrackedApp};
use headway_core::paths::display_path;
use headway_core::presets::PRESETS;
use headway_core::{AppView, Error, Headway, Result, SourceSpec};
use serde_json::{Value, json};

use super::{next_for, without_dry_run};
use crate::args::EditArgs;
use crate::fields;
use crate::human::{self, Palette};
use crate::output::Reply;

pub fn list(hw: &Headway, p: Palette) -> Result<Reply> {
    let views = hw.list()?;
    let behind: Vec<String> = views
        .iter()
        .filter(|view| view.has_update())
        .map(|view| format!("headway update {}", view.app.id))
        .collect();
    let next = if views.is_empty() {
        vec!["headway add <owner/repo>".to_owned()]
    } else if behind.is_empty() {
        vec!["headway check".to_owned()]
    } else {
        behind.into_iter().take(3).collect()
    };
    let data = json!({
        "apps": views.iter().map(summary).collect::<Vec<_>>(),
        "total": views.len(),
    });
    Ok(Reply::new(&data, human::apps(p, &views)).next(next))
}

/// The compact per-app record `list` prints (`show` has everything).
fn summary(view: &AppView) -> Value {
    let branch = match view.app.github().map(|github| &github.track) {
        Some(Track::Commit { branch }) => branch.clone().or_else(|| {
            view.last_check
                .as_ref()
                .and_then(|check| check.branch.clone())
        }),
        Some(Track::Release { .. } | Track::Tag { .. }) | None => None,
    };
    json!({
        "id": view.app.id,
        "name": view.app.name,
        "repo": view.app.github().map(|github| &github.repo),
        "source": view.app.source.kind(),
        "label": view.app.source.label(),
        "kind": view.app.kind,
        "mode": view.app.github().map(|github| github.track.mode()),
        "branch": branch,
        "method": view.app.source.method(),
        "status": view.status,
        "pending_count": view.pending_count,
        "busy": view.busy,
        "auto_update": view.app.auto_update,
        "update_policy": view.app.update_policy,
        "auto_update_paused": view.auto_update_paused,
        "installed": view.installed.as_ref().map(|installed| json!({
            "id": installed.id,
            "version": installed.version,
            "installed_at": installed.installed_at,
        })),
        "latest": view.last_check.as_ref().and_then(|check| check.latest.as_ref()).map(|latest| json!({
            "id": latest.id,
            "title": latest.title,
            "date": latest.date,
        })),
        "checked_at": view.last_check.as_ref().map(|check| check.checked_at),
        "error": view.last_error.as_ref().map(|error| json!({
            "code": error.code,
            "message": error.message,
            "during": error.during,
        })),
    })
}

pub fn show(hw: &Headway, id: &str, p: Palette) -> Result<Reply> {
    let view = hw.get(id)?;
    let history: Vec<_> = hw.history(id)?.into_iter().take(10).collect();
    let mut data = serde_json::to_value(&view).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut data {
        map.insert(
            "history".to_owned(),
            serde_json::to_value(&history).unwrap_or(Value::Null),
        );
    }
    let human = human::app_detail(p, &view, &history);
    Ok(Reply {
        data,
        human,
        next: next_for(&view),
        exit_code: 0,
    })
}

pub(super) fn toml_of(app: &TrackedApp) -> String {
    #[derive(serde::Serialize)]
    struct Wrapper<'a> {
        app: [&'a TrackedApp; 1],
    }
    toml::to_string_pretty(&Wrapper { app: [app] }).unwrap_or_default()
}

pub fn edit(hw: &Headway, args: &EditArgs, p: Palette) -> Result<Reply> {
    let current = hw.get(&args.id)?.app;
    let app = fields::apply(current.clone(), &args.fields)?;
    app.validate()?;
    let changed = app != current;
    if args.dry_run || !changed {
        let note = if changed {
            "dry run, nothing saved:"
        } else {
            "no changes:"
        };
        let human = format!("{}\n{}", p.dim(note), toml_of(&app));
        let next = if changed {
            vec![without_dry_run()]
        } else {
            Vec::new()
        };
        return Ok(Reply::new(
            &json!({"app": app, "changed": changed, "saved": false}),
            human,
        )
        .next(next));
    }
    let view = hw.replace(app)?;
    let id = view.app.id.clone();
    let human = format!(
        "{} updated `{id}`: follows {}, {}",
        p.green("✓"),
        human::source_mode(&view.app),
        human::source_install(&view.app)
    );
    Ok(Reply::new(
        &json!({"app": view.app, "changed": true, "saved": true}),
        human,
    )
    .next(vec![format!("headway check {id}")]))
}

pub fn remove(hw: &Headway, id: &str, purge: bool, dry_run: bool, p: Palette) -> Result<Reply> {
    hw.get(id)?;
    let paths = hw.paths();
    let mut targets = vec![format!(
        "the `{id}` entry in {}",
        display_path(&paths.config_file())
    )];
    if purge {
        targets.push(display_path(&paths.source_dir(id)));
        targets.push(display_path(&paths.app_log_dir(id)));
    }
    let human_list = targets
        .iter()
        .map(|item| format!("  - {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    if dry_run {
        let human = format!(
            "{}\n{human_list}\n  (the installed program itself is left alone)",
            p.dim("would remove:")
        );
        return Ok(Reply::new(
            &json!({"id": id, "would_remove": targets, "removed": false}),
            human,
        )
        .next(vec![without_dry_run()]));
    }
    let removed = hw.remove(id, purge)?;
    let human = format!(
        "{} stopped tracking `{id}` (the installed program is left alone)",
        p.green("✓")
    );
    Ok(Reply::new(
        &json!({"id": removed.id, "purged": removed.purged, "removed": true}),
        human,
    ))
}

pub async fn inspect(hw: &Headway, repo: &str, p: Palette) -> Result<Reply> {
    match SourceSpec::parse(repo)? {
        SourceSpec::Npm(_) | SourceSpec::Brew { .. } => {
            let found = hw.lookup(repo).await?;
            let human = format!("{}\n{}", found.package, toml_of(&found.suggestion));
            return Ok(Reply::new(&found, human).next(vec![format!(
                "headway add {}",
                headway_core::process::shell_quote(repo)
            )]));
        }
        SourceSpec::Script(_) => {
            return Err(Error::InvalidInput(
                "script sources use explicit --update-command; no inspection available".into(),
            ));
        }
        SourceSpec::Github(_) => {}
    }
    let found = hw.inspect(repo).await?;

    let human = human::inspection(p, &found);
    let next = vec![format!("headway add {}", found.repo)];
    Ok(Reply::new(&found, human).next(next))
}

pub fn presets(p: Palette) -> Reply {
    Reply::new(&json!({ "presets": PRESETS }), human::presets(p, PRESETS))
}

pub fn history(hw: &Headway, id: &str, limit: usize, p: Palette) -> Result<Reply> {
    let entries: Vec<_> = hw.history(id)?.into_iter().take(limit).collect();
    let human = if entries.is_empty() {
        format!("No installs recorded for `{id}` yet.")
    } else {
        human::history_lines(p, &entries, "")
    };
    Ok(Reply::new(&json!({ "id": id, "history": entries }), human))
}

pub fn log(hw: &Headway, id: &str, tail: Option<usize>) -> Result<Reply> {
    let Some(log) = hw.last_log(id)? else {
        let human = format!("No update has run for `{id}` yet.");
        return Ok(Reply::new(
            &json!({ "id": id, "path": Value::Null, "content": Value::Null }),
            human,
        )
        .next(vec![format!("headway update {id}")]));
    };
    let content = match tail {
        Some(count) => {
            let lines: Vec<&str> = log.content.lines().collect();
            lines[lines.len().saturating_sub(count)..].join("\n")
        }
        None => log.content.clone(),
    };
    let data = json!({ "id": id, "path": log.path, "content": content });
    Ok(Reply::new(&data, content))
}
