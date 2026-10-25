//! Repository identity and evidence selection for package candidates.
use super::{
    MatchEvidence, MatchEvidenceKind as Kind, PackageMatch, SourceMatches, evidence,
    metadata::PackageMeta, search,
};
use crate::{github::GitHub, repo::RepoId};

pub(super) struct Targets {
    pub repo: RepoId,
    repos: Vec<RepoId>,
    homepage: Option<String>,
    pub terms: Vec<String>,
}

impl Targets {
    pub(super) async fn resolve(
        repo: &RepoId,
        gh: Option<&GitHub>,
        report: &mut SourceMatches,
    ) -> Self {
        let mut targets = Self {
            repo: repo.clone(),
            repos: vec![repo.clone()],
            homepage: None,
            terms: search::terms(&repo.name),
        };
        if let Some(gh) = gh {
            match gh.repo(repo).await {
                Ok(info) => {
                    match RepoId::parse(&info.full_name) {
                        Ok(canonical) => {
                            targets.repo = canonical.clone();
                            targets.repos = vec![canonical];
                        }
                        Err(error) => report.error("github", &error),
                    }
                    targets.homepage = info.homepage;
                    for related in info.parent.iter().chain(&info.source) {
                        match RepoId::parse(&related.full_name) {
                            Ok(repo)
                                if !targets.repos.iter().any(|r| {
                                    r.full_name().eq_ignore_ascii_case(&repo.full_name())
                                }) =>
                            {
                                targets.repos.push(repo);
                            }
                            Ok(_) => {}
                            Err(error) => report.error("github", &error),
                        }
                    }
                    targets.terms = search::terms(&targets.repo.name);
                }
                Err(error) => report.error("github", &error),
            }
        }
        targets
    }

    pub(super) fn exact_name(&self, package: &str) -> bool {
        let short = search::short_name(package);
        short.eq_ignore_ascii_case(&self.repo.name)
            || self
                .terms
                .iter()
                .any(|term| short.eq_ignore_ascii_case(term))
    }

    pub(super) fn candidate(
        &self,
        package: PackageMeta,
        installed_index: bool,
    ) -> Option<PackageMatch> {
        let repositories = package.repositories();
        let target = self.repos.iter().find(|target| {
            repositories
                .iter()
                .any(|(repo, _)| repo.full_name().eq_ignore_ascii_case(&target.full_name()))
        });
        if target.is_none() && (installed_index || !repositories.is_empty()) {
            return None;
        }
        let repo = target.unwrap_or(&self.repo).full_name();
        let mut proof: Vec<_> = repositories
            .into_iter()
            .filter(|(r, _)| r.full_name().eq_ignore_ascii_case(&repo))
            .map(|(_, evidence)| evidence)
            .collect();
        proof.extend(evidence::homepage_match(
            package.homepage.as_deref(),
            self.homepage.as_deref(),
        ));
        if self.exact_name(&package.package) {
            proof.push(MatchEvidence {
                kind: Kind::Name,
                value: search::short_name(&package.package).into(),
            });
        }
        if proof.is_empty() {
            return None;
        }
        Some(PackageMatch {
            package: package.package,
            cask: package.cask,
            name: package.name,
            description: package.description,
            homepage: package.homepage,
            installed: package.installed,
            repo,
            confidence: evidence::confidence(&proof),
            evidence: evidence::ordered(proof),
            tracked_by: Vec::new(),
        })
    }
}
