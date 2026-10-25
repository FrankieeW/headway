use crate::{CancellationToken, Error, Result, probe, shell::ShellEnv, source::is_brew_name};

pub(super) fn short_name(package: &str) -> &str {
    package
        .rsplit('/')
        .next()
        .unwrap_or(package)
        .split('@')
        .next()
        .unwrap_or(package)
}

pub(super) fn valid_term(term: &str) -> bool {
    let mut chars = term.chars();
    term.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._+-".contains(c))
}

pub(super) fn terms(name: &str) -> Vec<String> {
    let name = name
        .strip_suffix(".git")
        .unwrap_or(name)
        .to_ascii_lowercase();
    let mut terms = vec![name.clone()];
    if let Some(stem) = [
        "-releases",
        "-desktop",
        "-app",
        "-cli",
        "-mac",
        "-macos",
        "-osx",
    ]
    .iter()
    .find_map(|suffix| name.strip_suffix(suffix))
    {
        terms.push(stem.into());
    }
    terms.into_iter().filter(|term| valid_term(term)).collect()
}

pub(super) fn rank_names(output: &str, terms: &[String]) -> Vec<String> {
    let mut names: Vec<_> = output
        .lines()
        .map(str::trim)
        .filter(|name| is_brew_name(name) && rank(name, terms).is_some())
        .map(str::to_owned)
        .collect();
    names.sort_by(|a, b| rank(a, terms).cmp(&rank(b, terms)).then_with(|| a.cmp(b)));
    names.dedup();
    names
}

fn rank(name: &str, terms: &[String]) -> Option<u8> {
    let name = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    terms
        .iter()
        .filter_map(|term| {
            if name == *term {
                Some(0)
            } else if name.starts_with(&format!("{term}@")) {
                Some(1)
            } else if name.starts_with(&format!("{term}-")) || name.ends_with(&format!("-{term}")) {
                Some(2)
            } else if name.contains(term) {
                Some(3)
            } else {
                None
            }
        })
        .min()
}

pub(super) async fn names(
    cancel: &CancellationToken,
    env: &ShellEnv,
    term: &str,
    cask: bool,
) -> Result<String> {
    if !valid_term(term) {
        return Err(Error::InvalidInput("invalid Homebrew search term".into()));
    }
    let cmd = super::probe::command(env).args([
        "search",
        if cask { "--cask" } else { "--formula" },
        term,
    ]);
    let out = probe::capture(cancel, &cmd).await?;
    if out.success() {
        return Ok(out.stdout);
    }
    if out.status == Some(1) && out.stderr.contains("No formulae or casks found") {
        return Ok(String::new());
    }
    Err(probe::failed(&cmd, &out))
}
