//! Managing tracked apps: list, show, add, edit, remove, inspect, history, log.

use std::fmt::Write as _;

use headway_core::config::{Install, SourceInstall, Track, TrackedApp};
use headway_core::inspect::RepoInspection;
use headway_core::paths::display_path;
use headway_core::presets::PRESETS;
use headway_core::repo::{RepoId, slugify};
use headway_core::{AppKind, AppView, Error, Headway, Result};
use serde_json::{Value, json};

use super::{next_for, without_dry_run};
use crate::args::{AddArgs, AppFields, EditArgs, KindArg, MethodArg, TrackArg};
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
    let branch = match &view.app.track {
        Track::Commit { branch } => branch.clone().or_else(|| {
            view.last_check
                .as_ref()
                .and_then(|check| check.branch.clone())
        }),
        Track::Release { .. } | Track::Tag { .. } => None,
    };
    json!({
        "id": view.app.id,
        "name": view.app.name,
        "repo": view.app.repo,
        "kind": view.app.kind,
        "mode": view.app.track.mode(),
        "branch": branch,
        "method": view.app.install.method(),
        "status": view.status,
        "pending_count": view.pending_count,
        "busy": view.busy,
        "auto_update": view.app.auto_update,
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

pub async fn add(hw: &Headway, args: &AddArgs, p: Palette) -> Result<Reply> {
    let repo = RepoId::parse(&args.repo)?;
    let (base, notes, fields) = if args.no_inspect {
        (bare_app(&repo), Vec::new(), args.fields.clone())
    } else {
        let found = hw.inspect(&args.repo).await?;
        let fields = prefer_release_asset(&args.fields, &found);
        (found.suggestion, found.notes, fields)
    };
    let base = match &args.id {
        Some(id) => TrackedApp {
            id: id.clone(),
            ..base
        },
        None => base,
    };
    let app = fields::apply(base.clone(), &fields)?;
    let overridden =
        app.track.mode() != base.track.mode() || app.install.method() != base.install.method();
    let notes = if overridden && !notes.is_empty() {
        vec![format!(
            "Flags override the suggested setup (follows {}, {}).",
            human::mode_label(&base.track),
            human::install_label(&base.install)
        )]
    } else {
        notes
    };
    app.validate().map_err(|err| with_add_hint(err, &app))?;

    let mut human = String::new();
    if args.dry_run {
        let _ = writeln!(human, "{}", p.dim("dry run, nothing saved:"));
        human.push_str(&toml_of(&app));
        append_notes(&mut human, &notes);
        let data = json!({"app": app, "notes": notes, "saved": false});
        return Ok(Reply::new(&data, human).next(vec![without_dry_run()]));
    }
    let view = hw.add(app)?;
    let id = view.app.id.clone();
    let _ = writeln!(
        human,
        "{} tracking {} as `{id}`: follows {}, {}",
        p.green("✓"),
        repo,
        human::mode_label(&view.app.track),
        human::install_label(&view.app.install)
    );
    append_notes(&mut human, &notes);
    let data = json!({"app": view.app, "notes": notes, "saved": true});
    Ok(Reply::new(&data, human).next(vec![
        format!("headway check {id}"),
        format!("headway update {id} --dry-run"),
        format!("headway mark {id} latest"),
    ]))
}

/// `--track release` without `--method` means "download it" when the release has a macOS asset.
fn prefer_release_asset(fields: &AppFields, found: &RepoInspection) -> AppFields {
    let asset_kind = found
        .latest_release
        .as_ref()
        .and_then(|release| release.suggested_kind);
    match (fields.track, fields.method, asset_kind) {
        (Some(TrackArg::Release), None, Some(kind)) => AppFields {
            method: Some(MethodArg::Asset),
            kind: fields.kind.or(Some(match kind {
                AppKind::Cli => KindArg::Cli,
                AppKind::App => KindArg::App,
            })),
            ..fields.clone()
        },
        _ => fields.clone(),
    }
}

/// The starting point for `add --no-inspect`: everything must come from flags.
fn bare_app(repo: &RepoId) -> TrackedApp {
    TrackedApp {
        id: slugify(&repo.name),
        name: repo.name.clone(),
        repo: repo.full_name(),
        kind: AppKind::Cli,
        auto_update: false,
        git_url: None,
        track: Track::Commit { branch: None },
        install: Install::Source(SourceInstall::default()),
    }
}

fn with_add_hint(err: Error, app: &TrackedApp) -> Error {
    match (&err, &app.install) {
        (Error::Config(message), Install::Source(source)) if source.build.trim().is_empty() => {
            Error::InvalidInput(format!(
                "{message}; pass --build '<command>' or --preset <id> (see `headway presets`), or --track release --method asset"
            ))
        }
        _ => err,
    }
}

fn append_notes(out: &mut String, notes: &[String]) {
    for note in notes {
        let _ = writeln!(out, "  • {note}");
    }
}

fn toml_of(app: &TrackedApp) -> String {
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
        human::mode_label(&view.app.track),
        human::install_label(&view.app.install)
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
