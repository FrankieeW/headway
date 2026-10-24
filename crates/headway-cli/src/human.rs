//! Text rendering for people: colors, tables and per-command layouts.

use chrono::{DateTime, Utc};
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

#[path = "human_apps.rs"]
mod app_render;
#[path = "human_operations.rs"]
mod operations;
pub use app_render::{app_detail, apps, source_install, source_mode, status_cell};
pub use operations::{doctor, history_lines, inspection, outcome, plan, presets};
