//! Text rendering for people: colors, tables and per-command layouts.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use headway_core::doctor::{CheckStatus, DoctorReport};
use headway_core::inspect::RepoInspection;
use headway_core::presets::Preset;
use headway_core::update::short;
use headway_core::{AppStatus, AppView, HistoryEntry, Install, Track, UpdateOutcome, UpdatePlan};
use unicode_width::UnicodeWidthStr as _;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub color: bool,
}

impl Palette {
    fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_owned()
        }
    }

    pub fn bold(self, text: &str) -> String {
        self.paint("1", text)
    }

    pub fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }

    pub fn red(self, text: &str) -> String {
        self.paint("31", text)
    }

    pub fn green(self, text: &str) -> String {
        self.paint("32", text)
    }

    pub fn yellow(self, text: &str) -> String {
        self.paint("33", text)
    }

    pub fn cyan(self, text: &str) -> String {
        self.paint("36", text)
    }
}

/// A table whose cells are styled after padding, so colors don't break alignment.
pub fn table(p: Palette, headers: &[&str], rows: &[Vec<(String, Style)>]) -> String {
    let columns = headers.len();
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|(text, _)| text.width())
                .chain(std::iter::once(headers[column].width()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<(String, Style)>| {
        let rendered: Vec<String> = cells
            .into_iter()
            .enumerate()
            .map(|(column, (text, style))| {
                let last = column + 1 == columns;
                let pad = if last {
                    0
                } else {
                    widths[column].saturating_sub(text.width())
                };
                format!("{}{}", style.apply(p, &text), " ".repeat(pad))
            })
            .collect();
        rendered.join("  ").trim_end().to_owned()
    };
    let header = line(
        headers
            .iter()
            .map(|h| ((*h).to_owned(), Style::Dim))
            .collect(),
    );
    std::iter::once(header)
        .chain(rows.iter().map(|row| line(row.clone())))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Bold,
    Dim,
    Green,
    Yellow,
    Red,
    Cyan,
}

impl Style {
    pub fn apply(self, p: Palette, text: &str) -> String {
        match self {
            Self::Plain => text.to_owned(),
            Self::Bold => p.bold(text),
            Self::Dim => p.dim(text),
            Self::Green => p.green(text),
            Self::Yellow => p.yellow(text),
            Self::Red => p.red(text),
            Self::Cyan => p.cyan(text),
        }
    }
}

/// `3m ago`, `5h ago`, `2d ago`, or a date.
pub fn ago(at: DateTime<Utc>) -> String {
    let seconds = (Utc::now() - at).num_seconds().max(0);
    match seconds {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86_400 => format!("{}h ago", seconds / 3600),
        86_400..2_592_000 => format!("{}d ago", seconds / 86_400),
        _ => at.format("%Y-%m-%d").to_string(),
    }
}

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
                unit(&view.app.track, view.pending_count)
            ),
            Style::Yellow,
        ),
        AppStatus::NotInstalled => ("○ not installed".to_owned(), Style::Dim),
        AppStatus::Diverged => ("⚠ diverged".to_owned(), Style::Yellow),
        AppStatus::Error => ("✗ error".to_owned(), Style::Red),
        AppStatus::Busy => ("⟳ updating".to_owned(), Style::Cyan),
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
                (view.app.repo.clone(), Style::Plain),
                (mode_label(&view.app.track), Style::Dim),
                (installed_label(view), Style::Plain),
                status_cell(view),
            ]
        })
        .collect();
    table(p, &["ID", "REPO", "FOLLOWS", "INSTALLED", "STATUS"], &rows)
}

pub fn app_detail(p: Palette, view: &AppView, history: &[HistoryEntry]) -> String {
    let app = &view.app;
    let mut out = String::new();
    let (status, style) = status_cell(view);
    let _ = writeln!(
        out,
        "{}  {}  {}",
        p.bold(&app.name),
        p.dim(&app.repo),
        style.apply(p, &status)
    );
    let _ = writeln!(out, "  id         {}", app.id);
    let _ = writeln!(out, "  follows    {}", mode_label(&app.track));
    let _ = writeln!(out, "  install    {}", install_label(&app.install));
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

pub fn history_lines(p: Palette, history: &[HistoryEntry], indent: &str) -> String {
    history.iter().fold(String::new(), |mut out, entry| {
        let mark = if entry.ok {
            p.green("✓")
        } else {
            p.red("✗")
        };
        let action = match entry.action {
            headway_core::state::HistoryAction::Update => "update",
            headway_core::state::HistoryAction::Mark => "mark",
        };
        let from = entry
            .from
            .as_deref()
            .map(|from| format!("{} → ", short(from)))
            .unwrap_or_default();
        let version = entry
            .version
            .as_deref()
            .map(|version| format!(" ({version})"))
            .unwrap_or_default();
        let took = entry
            .duration_ms
            .map(|ms| {
                format!(
                    " in {:.1}s",
                    f64::from(u32::try_from(ms).unwrap_or(u32::MAX)) / 1000.0
                )
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "{indent}{mark} {action:<6} {from}{}{version}  {}",
            short(&entry.to),
            p.dim(&format!("{}{took}", ago(entry.at)))
        );
        if let Some(message) = entry.message.as_deref().filter(|_| !entry.ok) {
            let _ = writeln!(out, "{indent}         {}", p.dim(message));
        }
        out
    })
}

pub fn plan(p: Palette, plan: &UpdatePlan) -> String {
    let mut out = String::new();
    let from = plan
        .from
        .as_ref()
        .map_or_else(|| "nothing".to_owned(), |from| short(&from.id));
    let _ = writeln!(
        out,
        "{} {from} → {}  {}",
        p.bold(&plan.app_id),
        p.yellow(&short(&plan.to.id)),
        plan.to.title
    );
    if plan.up_to_date {
        let _ = writeln!(
            out,
            "  {}",
            p.green("already installed: a real run would do nothing")
        );
        return out;
    }
    for (index, step) in plan.steps.iter().enumerate() {
        let _ = writeln!(out, "  {}. {step}", index + 1);
    }
    out
}

pub fn outcome(p: Palette, outcome: &UpdateOutcome) -> String {
    if !outcome.changed {
        return format!(
            "{} {} is already installed",
            p.green("✓"),
            p.bold(&outcome.app_id)
        );
    }
    let from = outcome
        .from
        .as_ref()
        .map(|from| format!("{} → ", short(&from.id)))
        .unwrap_or_default();
    let version = outcome
        .to
        .version
        .as_deref()
        .filter(|version| *version != outcome.to.id)
        .map(|version| format!(" ({version})"))
        .unwrap_or_default();
    let place = outcome
        .installed_path
        .as_deref()
        .map(|path| format!(" → {path}"))
        .unwrap_or_default();
    format!(
        "{} {} {from}{}{version}{place}  {}",
        p.green("✓"),
        p.bold(&outcome.app_id),
        short(&outcome.to.id),
        p.dim(&format!(
            "{:.1}s",
            f64::from(u32::try_from(outcome.duration_ms).unwrap_or(u32::MAX)) / 1000.0
        ))
    )
}

pub fn inspection(p: Palette, found: &RepoInspection) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}  {}",
        p.bold(&found.repo),
        p.dim(found.description.as_deref().unwrap_or(""))
    );
    let flags = [(found.private, "private"), (found.archived, "archived")]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, label)| *label)
        .collect::<Vec<_>>();
    let _ = writeln!(
        out,
        "  default branch  {}{}",
        found.default_branch,
        if flags.is_empty() {
            String::new()
        } else {
            format!("  ({})", flags.join(", "))
        }
    );
    let _ = writeln!(
        out,
        "  build system    {}",
        found.build_system.as_deref().unwrap_or("not detected")
    );
    if !found.binaries.is_empty() {
        let _ = writeln!(out, "  binaries        {}", found.binaries.join(", "));
    }
    if let Some(release) = &found.latest_release {
        let asset = release
            .suggested_asset
            .as_deref()
            .unwrap_or("no macOS asset");
        let _ = writeln!(out, "  latest release  {}  ({asset})", release.tag);
    }
    let _ = writeln!(
        out,
        "\n  suggestion: follows {}, {}",
        mode_label(&found.suggestion.track),
        install_label(&found.suggestion.install)
    );
    for note in &found.notes {
        let _ = writeln!(out, "  • {note}");
    }
    out
}

pub fn presets(p: Palette, presets: &[Preset]) -> String {
    let rows: Vec<Vec<(String, Style)>> = presets
        .iter()
        .map(|preset| {
            vec![
                (preset.id.to_owned(), Style::Bold),
                (format!("{:?}", preset.kind).to_lowercase(), Style::Dim),
                (
                    if preset.build.is_empty() {
                        "—".to_owned()
                    } else {
                        preset.build.to_owned()
                    },
                    Style::Plain,
                ),
            ]
        })
        .collect();
    table(p, &["PRESET", "KIND", "BUILD"], &rows)
}

pub fn doctor(p: Palette, report: &DoctorReport) -> String {
    report.checks.iter().fold(String::new(), |mut out, check| {
        let mark = match check.status {
            CheckStatus::Ok => p.green("✓"),
            CheckStatus::Warn => p.yellow("!"),
            CheckStatus::Error => p.red("✗"),
        };
        let _ = writeln!(out, "{mark} {:<12} {}", check.name, check.message);
        if let Some(hint) = &check.hint {
            let _ = writeln!(out, "  {:<12} {}", "", p.dim(&format!("→ {hint}")));
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_align_wide_characters() {
        let p = Palette { color: false };
        let rows = vec![
            vec![
                ("工具".to_owned(), Style::Plain),
                ("x".to_owned(), Style::Plain),
            ],
            vec![
                ("ab".to_owned(), Style::Plain),
                ("y".to_owned(), Style::Plain),
            ],
        ];
        let text = table(p, &["NAME", "V"], &rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "NAME  V");
        assert_eq!(lines[1], "工具  x");
        assert_eq!(lines[2], "ab    y");
    }

    #[test]
    fn colors_only_when_enabled() {
        assert_eq!(Palette { color: false }.red("x"), "x");
        assert_eq!(Palette { color: true }.red("x"), "\u{1b}[31mx\u{1b}[0m");
    }

    #[test]
    fn relative_times() {
        let now = Utc::now();
        assert_eq!(ago(now), "just now");
        assert_eq!(ago(now - chrono::Duration::minutes(5)), "5m ago");
        assert_eq!(ago(now - chrono::Duration::hours(3)), "3h ago");
        assert_eq!(ago(now - chrono::Duration::days(2)), "2d ago");
    }
}
