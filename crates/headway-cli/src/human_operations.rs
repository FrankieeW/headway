//! Operation and diagnostic rendering.
use super::{Palette, Style, ago, source_install, source_mode, table};
use headway_core::doctor::{CheckStatus, DoctorReport};
use headway_core::inspect::RepoInspection;
use headway_core::presets::Preset;
use headway_core::update::short;
use headway_core::{HistoryEntry, UpdateOutcome, UpdatePlan};
use std::fmt::Write as _;
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
    if plan.reinstall {
        let _ = writeln!(
            out,
            "  {}",
            p.yellow("already installed: a real run reinstalls it")
        );
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
    for candidate in &found.build_candidates {
        let line = candidate
            .line
            .map(|line| format!(":{line}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  build candidate {}{}{}\n    {}",
            candidate.file,
            line,
            if candidate.recommended {
                " (recommended)"
            } else {
                ""
            },
            candidate.build.replace('\n', "\n    ")
        );
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
        source_mode(&found.suggestion),
        source_install(&found.suggestion)
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
