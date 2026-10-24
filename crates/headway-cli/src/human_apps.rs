//! App status and source rendering.
use super::{Palette, Style, ago, history_lines, table};
use headway_core::update::short;
use headway_core::{AppStatus, AppView, HistoryEntry, Install, Track};
use std::fmt::Write as _;
pub fn mode_label(track: &Track) -> String {
    match track {
        Track::Commit {
            branch: Some(branch),
        } => format!("commit · {branch}"),
        Track::Commit { branch: None } => "commit".to_owned(),
        Track::Release { prerelease, .. } => if *prerelease {
            "release+pre"
        } else {
            "release"
        }
        .to_owned(),
        Track::Tag { .. } => "tag".to_owned(),
    }
}

fn unit(track: &Track, count: u32) -> &'static str {
    match (track, count == 1) {
        (Track::Commit { .. }, true) => "commit",
        (Track::Commit { .. }, false) => "commits",
        (Track::Release { .. }, true) => "release",
        (Track::Release { .. }, false) => "releases",
        (Track::Tag { .. }, true) => "tag",
        (Track::Tag { .. }, false) => "tags",
    }
}

pub fn status_cell(view: &AppView) -> (String, Style) {
    match view.status {
        AppStatus::UpToDate => ("✓ up to date".to_owned(), Style::Green),
        AppStatus::UpdateAvailable => (
            format!(
                "↑ {} new {}",
                view.pending_count,
                view.app
                    .github()
                    .map_or("versions", |github| unit(&github.track, view.pending_count))
            ),
            Style::Yellow,
        ),
        AppStatus::NotInstalled => ("○ not installed".to_owned(), Style::Dim),
        AppStatus::Diverged => ("⚠ diverged".to_owned(), Style::Yellow),
        AppStatus::Error => ("✗ error".to_owned(), Style::Red),
        AppStatus::Busy => ("⟳ updating".to_owned(), Style::Cyan),
        AppStatus::Unknown => ("? version unknown".to_owned(), Style::Dim),
        AppStatus::Unchecked => ("· not checked".to_owned(), Style::Dim),
    }
}

fn installed_label(view: &AppView) -> String {
    view.installed.as_ref().map_or_else(
        || "—".to_owned(),
        |installed| match &installed.version {
            Some(version) if version != &installed.id && installed.sha.is_some() => {
                format!("{} ({version})", short(&installed.id))
            }
            Some(version) => version.clone(),
            None => short(&installed.id),
        },
    )
}

pub fn apps(p: Palette, views: &[AppView]) -> String {
    if views.is_empty() {
        return "No apps tracked yet. Start with `headway add owner/repo`.".to_owned();
    }
    let rows: Vec<Vec<(String, Style)>> = views
        .iter()
        .map(|view| {
            vec![
                (view.app.id.clone(), Style::Bold),
                (view.app.source.label(), Style::Plain),
                (source_mode(&view.app), Style::Dim),
                (installed_label(view), Style::Plain),
                status_cell(view),
            ]
        })
        .collect();
    table(
        p,
        &["ID", "SOURCE", "FOLLOWS", "INSTALLED", "STATUS"],
        &rows,
    )
}

pub fn app_detail(p: Palette, view: &AppView, history: &[HistoryEntry]) -> String {
    let app = &view.app;
    let mut out = String::new();
    let (status, style) = status_cell(view);
    let _ = writeln!(
        out,
        "{}  {}  {}",
        p.bold(&app.name),
        p.dim(&app.source.label()),
        style.apply(p, &status)
    );
    let _ = writeln!(out, "  id         {}", app.id);
    let _ = writeln!(out, "  follows    {}", source_mode(app));
    let _ = writeln!(out, "  install    {}", source_install(app));
    let _ = writeln!(out, "  installed  {}", installed_label(view));
    if let Some(installed) = &view.installed {
        let _ = writeln!(
            out,
            "             {}",
            p.dim(&format!("at {}", ago(installed.installed_at)))
        );
    }
    if let Some(check) = &view.last_check {
        if let Some(latest) = &check.latest {
            let _ = writeln!(out, "  latest     {}  {}", short(&latest.id), latest.title);
        }
        let _ = writeln!(out, "  checked    {}", ago(check.checked_at));
        if !check.pending.is_empty() {
            let heading = if view.installed.is_some() {
                "newer"
            } else {
                "recent"
            };
            let _ = writeln!(out, "\n  {heading}:");
            for candidate in check.pending.iter().take(15) {
                let when = candidate.date.map(ago).unwrap_or_default();
                let _ = writeln!(
                    out,
                    "    {}  {}  {}",
                    p.yellow(&short(&candidate.id)),
                    candidate.title,
                    p.dim(&when)
                );
            }
            let more = check.pending.len().saturating_sub(15)
                + usize::try_from(check.pending_total)
                    .unwrap_or(0)
                    .saturating_sub(check.pending.len());
            if more > 0 {
                let _ = writeln!(out, "    {}", p.dim(&format!("… and {more} more")));
            }
        }
    }
    if let Some(error) = &view.last_error {
        let _ = writeln!(
            out,
            "\n  {} ({}) {}",
            p.red("last error"),
            error.during,
            error.message
        );
    }
    if !history.is_empty() {
        let _ = writeln!(out, "\n  history:");
        out.push_str(&history_lines(p, history, "    "));
    }
    out
}

pub fn install_label(install: &Install) -> String {
    match install {
        Install::Source(source) => {
            let target = source
                .artifact
                .as_deref()
                .unwrap_or("(build installs itself)");
            format!("build `{}` → {target}", source.build)
        }
        Install::Asset(asset) => format!(
            "release asset {}",
            asset
                .pattern
                .as_deref()
                .unwrap_or("(auto: macOS, this CPU)")
        ),
    }
}

pub fn source_mode(app: &headway_core::TrackedApp) -> String {
    match &app.source {
        headway_core::Source::Github(github) => mode_label(&github.track),
        headway_core::Source::Npm(npm) => npm.dist_tag.clone(),
        headway_core::Source::Brew(_) => "stable".into(),
        headway_core::Source::Script(_) => "official updater".into(),
    }
}

pub fn source_install(app: &headway_core::TrackedApp) -> String {
    match &app.source {
        headway_core::Source::Github(github) => install_label(&github.install),
        headway_core::Source::Npm(_) | headway_core::Source::Brew(_) => app.source.label(),
        headway_core::Source::Script(script) => script.update_command.clone(),
    }
}
