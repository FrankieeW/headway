//! Human output and next actions for read-only source matching.
use crate::output::Reply;
use headway_core::{
    Headway, MatchConfidence, MatchEvidence, MatchEvidenceKind, MatchSide, Result,
    process::shell_quote,
};

pub async fn matching(hw: &Headway, input: &str, offline: bool) -> Result<Reply> {
    let report = hw.match_source(input, offline).await?;
    let mut lines = vec![format!(
        "{} → {}",
        clean(&report.query),
        match report.from {
            MatchSide::Brew => "GitHub",
            MatchSide::Github => "Homebrew",
        }
    )];
    let mut next = Vec::new();
    for repo in &report.repositories {
        lines.push(format!(
            "{}  {}",
            clean(&repo.repo),
            details(repo.confidence, &repo.evidence, &repo.tracked_by)
        ));
        next.extend(hints(&repo.repo, repo.confidence, &repo.tracked_by));
    }
    for package in &report.packages {
        let kind = if package.cask { "cask" } else { "formula" };
        let installed = package
            .installed
            .as_ref()
            .map_or_else(String::new, |v| format!("  installed {}", clean(v)));
        lines.push(format!(
            "{kind} {}  {}{installed}",
            clean(&package.package),
            details(package.confidence, &package.evidence, &package.tracked_by)
        ));
        let target = format!(
            "{}:{}",
            if package.cask { "cask" } else { "brew" },
            package.package
        );
        next.extend(hints(&target, package.confidence, &package.tracked_by));
    }
    if report.repositories.is_empty() && report.packages.is_empty() {
        lines.push("No match found.".into());
    }
    lines.extend(report.notes.iter().map(|note| clean(note)));
    lines.extend(
        report
            .errors
            .iter()
            .map(|issue| format!("{}: {}", clean(&issue.source), clean(&issue.error.message))),
    );
    let next = unique_hints(next);
    let exit = u8::from(!report.errors.is_empty());
    Ok(Reply::new(&report, lines.join("\n"))
        .next(next)
        .exit_code(exit))
}

fn unique_hints(hints: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for hint in hints {
        if !result.contains(&hint) {
            result.push(hint);
        }
        if result.len() == 3 {
            break;
        }
    }
    result
}

fn hints(target: &str, confidence: MatchConfidence, tracked: &[String]) -> Vec<String> {
    if !tracked.is_empty() {
        return tracked
            .iter()
            .map(|id| format!("headway show {}", shell_quote(id)))
            .collect();
    }
    let verb = match confidence {
        MatchConfidence::High | MatchConfidence::Medium => "add",
        MatchConfidence::Low => "inspect",
    };
    vec![format!("headway {verb} {}", shell_quote(target))]
}

fn details(confidence: MatchConfidence, evidence: &[MatchEvidence], tracked: &[String]) -> String {
    let confidence = match confidence {
        MatchConfidence::High => "high",
        MatchConfidence::Medium => "medium",
        MatchConfidence::Low => "low",
    };
    let kinds = evidence
        .iter()
        .map(|item| match item.kind {
            MatchEvidenceKind::SourceUrl => "source_url",
            MatchEvidenceKind::HeadUrl => "head_url",
            MatchEvidenceKind::DownloadUrl => "download_url",
            MatchEvidenceKind::Homepage => "homepage",
            MatchEvidenceKind::HomepageMatch => "homepage_match",
            MatchEvidenceKind::Name => "name",
            MatchEvidenceKind::Search => "search",
        })
        .collect::<Vec<_>>()
        .join(", ");
    let tracked = if tracked.is_empty() {
        String::new()
    } else {
        format!("  tracked by {}", clean(&tracked.join(", ")))
    };
    format!("{confidence}  [{kinds}]{tracked}")
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_inspect_low_confidence_quote_targets_and_show_tracked() {
        assert_eq!(
            hints("acme/tool", MatchConfidence::Low, &[]),
            ["headway inspect acme/tool"]
        );
        assert_eq!(
            hints("cask:tool", MatchConfidence::High, &[]),
            ["headway add cask:tool"]
        );
        assert_eq!(
            hints("acme/tool", MatchConfidence::Medium, &["custom-id".into()]),
            ["headway show custom-id"]
        );
        assert_eq!(
            hints("a'b", MatchConfidence::High, &[]),
            ["headway add 'a'\\''b'"]
        );
        assert_eq!(
            unique_hints(vec![
                "a".into(),
                "a".into(),
                "b".into(),
                "c".into(),
                "d".into()
            ]),
            ["a", "b", "c"]
        );
    }
    #[test]
    fn human_matching_replaces_terminal_controls() {
        assert_eq!(clean("name\n\u{1b}[31m\tend"), "name??[31m?end");
    }
}
