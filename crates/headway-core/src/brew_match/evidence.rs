use super::{MatchConfidence, MatchEvidence, MatchEvidenceKind as Kind, RepositoryMatch};
use crate::{Source, TrackedApp, repo::RepoId};

pub(super) fn ordered(mut evidence: Vec<MatchEvidence>) -> Vec<MatchEvidence> {
    evidence.sort();
    evidence.dedup();
    evidence
}

pub(super) fn confidence(evidence: &[MatchEvidence]) -> MatchConfidence {
    evidence
        .iter()
        .map(|item| match item.kind {
            Kind::SourceUrl | Kind::HeadUrl | Kind::DownloadUrl | Kind::Homepage => {
                MatchConfidence::High
            }
            Kind::HomepageMatch => MatchConfidence::Medium,
            Kind::Name | Kind::Search => MatchConfidence::Low,
        })
        .min()
        .unwrap_or(MatchConfidence::Low)
}

pub(crate) fn repositories(items: Vec<(RepoId, MatchEvidence)>) -> Vec<RepositoryMatch> {
    let mut matches: Vec<RepositoryMatch> = Vec::new();
    for (repo, item) in items {
        let name = repo.full_name();
        if let Some(found) = matches
            .iter_mut()
            .find(|found| found.repo.eq_ignore_ascii_case(&name))
        {
            found.evidence.push(item);
        } else {
            matches.push(RepositoryMatch {
                repo: name,
                html_url: repo.html_url(),
                confidence: MatchConfidence::High,
                evidence: vec![item],
                description: None,
                stars: None,
                tracked_by: Vec::new(),
            });
        }
    }
    for found in &mut matches {
        found.evidence = ordered(std::mem::take(&mut found.evidence));
        found.confidence = confidence(&found.evidence);
    }
    matches.sort_by(|a, b| {
        a.repo
            .to_ascii_lowercase()
            .cmp(&b.repo.to_ascii_lowercase())
    });
    matches
}

pub(crate) fn tracked_repositories(matches: &mut [RepositoryMatch], apps: &[TrackedApp]) {
    for found in matches {
        found.tracked_by = apps
            .iter()
            .filter(|app| match &app.source {
                Source::Github(source) => source
                    .repo_id()
                    .is_ok_and(|repo| repo.full_name().eq_ignore_ascii_case(&found.repo)),
                Source::Npm(_) | Source::Brew(_) | Source::Script(_) => false,
            })
            .map(|app| app.id.clone())
            .collect();
    }
}

pub(super) fn homepage_match(
    package: Option<&str>,
    repository: Option<&str>,
) -> Option<MatchEvidence> {
    let (package, repository) = (package?, repository?);
    let a = super::urls::normalize_homepage(package)?;
    let b = super::urls::normalize_homepage(repository)?;
    (a == b)
        .then(|| super::urls::sanitize_url(package))
        .flatten()
        .map(|value| MatchEvidence {
            kind: Kind::HomepageMatch,
            value,
        })
}
