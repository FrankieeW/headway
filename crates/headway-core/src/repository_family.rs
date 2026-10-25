//! Fork ancestry comes from GitHub metadata, never from matching repository names.
use crate::{RepositoryOption, Result, github::RepoInfo, repo::RepoId};

pub(crate) fn choices(info: &RepoInfo) -> Result<Vec<RepositoryOption>> {
    let mut choices = Vec::new();
    for (name, branch) in std::iter::once((&info.full_name, &info.default_branch))
        .chain(
            info.parent
                .iter()
                .map(|repo| (&repo.full_name, &repo.default_branch)),
        )
        .chain(
            info.source
                .iter()
                .map(|repo| (&repo.full_name, &repo.default_branch)),
        )
    {
        let repo = RepoId::parse(name)?.full_name();
        if choices
            .iter()
            .any(|entry: &RepositoryOption| entry.repo.eq_ignore_ascii_case(&repo))
        {
            continue;
        }
        choices.push(RepositoryOption {
            label: repo.clone(),
            repo,
            branch: Some(branch.clone()),
            ..RepositoryOption::default()
        });
    }
    Ok(choices)
}
