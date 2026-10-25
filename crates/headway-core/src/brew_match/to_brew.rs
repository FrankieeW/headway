use super::{MatchSide, SourceMatches, metadata::PackageMeta, probe, search, targets::Targets};
use crate::{CancellationToken, Error, Result, github::GitHub, repo::RepoId, shell::ShellEnv};

pub(crate) async fn github_to_brew(
    cancel: &CancellationToken,
    env: &ShellEnv,
    gh: Option<&GitHub>,
    repo: &RepoId,
    offline: bool,
) -> Result<SourceMatches> {
    let mut report = SourceMatches::new(repo.full_name(), MatchSide::Github);
    let targets = Targets::resolve(repo, if offline { None } else { gh }, &mut report).await;
    match probe::installed(cancel, env).await {
        Ok(packages) => report.packages.extend(
            packages
                .into_iter()
                .filter_map(|package| targets.candidate(package, true)),
        ),
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(error) => report.error("brew", &error),
    }
    for cask in [false, true] {
        let mut output = String::new();
        for term in &targets.terms {
            match search::names(cancel, env, term, cask).await {
                Ok(names) => {
                    output.push_str(&names);
                    output.push('\n');
                }
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error) => report.error("brew", &error),
            }
        }
        let names: Vec<_> = search::rank_names(&output, &targets.terms)
            .into_iter()
            .filter(|name| {
                !report
                    .packages
                    .iter()
                    .any(|m| m.cask == cask && m.package == *name)
            })
            .take(10)
            .collect();
        if names.is_empty() {
            continue;
        }
        let verified = verify(cancel, env, &names, cask, &mut report).await?;
        for package in verified {
            if report
                .packages
                .iter()
                .any(|m| m.package == package.package && m.cask == package.cask)
            {
                continue;
            }
            if let Some(found) = targets.candidate(package, false) {
                report.packages.push(found);
            }
        }
    }
    report.packages.sort_by(|a, b| {
        a.confidence
            .cmp(&b.confidence)
            .then_with(|| b.installed.is_some().cmp(&a.installed.is_some()))
            .then_with(|| {
                targets
                    .exact_name(&b.package)
                    .cmp(&targets.exact_name(&a.package))
            })
            .then_with(|| a.package.cmp(&b.package))
            .then_with(|| a.cask.cmp(&b.cask))
    });
    report.packages.truncate(10);
    Ok(report)
}

async fn verify(
    cancel: &CancellationToken,
    env: &ShellEnv,
    names: &[String],
    cask: bool,
    report: &mut SourceMatches,
) -> Result<Vec<PackageMeta>> {
    match probe::info(cancel, env, names, cask).await {
        Ok(packages) => return Ok(packages),
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(_) => {} // Homebrew discards a whole batch when any name is unavailable.
    }
    let mut packages = Vec::new();
    for name in names {
        match probe::info(cancel, env, std::slice::from_ref(name), cask).await {
            Ok(found) if found.is_empty() => report.error("brew", &probe::missing(name, cask)),
            Ok(found) => packages.extend(found),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(error) => {
                let mut info = crate::ErrorInfo::from(&error);
                info.message = format!(
                    "Homebrew {} `{name}`: {}",
                    if cask { "cask" } else { "formula" },
                    info.message
                );
                report.errors.push(super::MatchError {
                    source: "brew".into(),
                    error: info,
                });
            }
        }
    }
    Ok(packages)
}
