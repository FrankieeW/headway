use super::{
    MatchConfidence, MatchEvidence, MatchEvidenceKind as Kind, MatchSide, RepositoryMatch,
    SourceMatches, evidence, metadata::PackageMeta, probe, search::short_name,
};
use crate::{
    BrewSource, CancellationToken, Result, Source, github::GitHub, repo::RepoId, shell::ShellEnv,
};

pub(crate) async fn brew_to_github(
    cancel: &CancellationToken,
    env: &ShellEnv,
    gh: Option<&GitHub>,
    source: &BrewSource,
    offline: bool,
) -> Result<SourceMatches> {
    let packages = probe::info(
        cancel,
        env,
        std::slice::from_ref(&source.package),
        source.cask,
    )
    .await?;
    let package = packages
        .first()
        .ok_or_else(|| probe::missing(&source.package, source.cask))?;
    let mut report = SourceMatches::new(Source::Brew(source.clone()).label(), MatchSide::Brew);
    report.repositories = evidence::repositories(package.repositories());
    if !report.repositories.is_empty() {
        return Ok(report);
    }
    if offline {
        report.notes.push("GitHub search skipped (offline).".into());
    } else if let Some(gh) = gh {
        search(gh, package, &mut report).await;
    }
    Ok(report)
}

async fn search(gh: &GitHub, package: &PackageMeta, report: &mut SourceMatches) {
    let short = short_name(&package.package);
    let mut terms = vec![short.to_owned()];
    let display = display_term(&package.name);
    let comparable = |name: &str| {
        name.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
    };
    if package.cask && !display.is_empty() && comparable(short) != comparable(&display) {
        terms.push(display);
    }
    let mut results = Vec::new();
    for term in terms {
        match gh.search_repositories(&term, 5).await {
            Ok(items) => results.push((term, items.into_iter())),
            Err(error) => report.error("github", &error),
        }
    }
    for _ in 0..5 {
        for (term, items) in &mut results {
            if let Some(item) = items.next() {
                let Ok(repo) = RepoId::parse(&item.full_name) else {
                    continue;
                };
                let mut proof = vec![MatchEvidence {
                    kind: Kind::Search,
                    value: term.clone(),
                }];
                if repo.name.eq_ignore_ascii_case(short) {
                    proof.push(MatchEvidence {
                        kind: Kind::Name,
                        value: short.into(),
                    });
                }
                proof.extend(evidence::homepage_match(
                    package.homepage.as_deref(),
                    item.homepage.as_deref(),
                ));
                if let Some(found) = report
                    .repositories
                    .iter_mut()
                    .find(|m| m.repo.eq_ignore_ascii_case(&repo.full_name()))
                {
                    found.evidence.extend(proof);
                    found.evidence = evidence::ordered(std::mem::take(&mut found.evidence));
                    found.confidence = evidence::confidence(&found.evidence);
                } else {
                    report.repositories.push(RepositoryMatch {
                        repo: repo.full_name(),
                        html_url: repo.html_url(),
                        confidence: evidence::confidence(&proof),
                        evidence: evidence::ordered(proof),
                        description: item.description,
                        stars: Some(item.stargazers_count),
                        tracked_by: Vec::new(),
                    });
                }
            }
        }
    }
    report.repositories.sort_by_key(|found| found.confidence);
    if report
        .repositories
        .iter()
        .any(|found| found.confidence != MatchConfidence::Low)
    {
        report
            .repositories
            .retain(|found| found.confidence != MatchConfidence::Low);
        report.repositories.truncate(5);
    } else {
        report.repositories.truncate(3);
    }
}

fn display_term(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " .-_+".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let mut term = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    term.truncate(64);
    term.trim_end().to_owned()
}
