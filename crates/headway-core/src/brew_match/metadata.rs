//! Minimal typed Homebrew metadata used for matching and inventory evidence.
use super::{
    MatchEvidence, MatchEvidenceKind as Kind,
    urls::{github_repo_in_url, sanitize_url},
};
use crate::{Error, Result, repo::RepoId, source::is_brew_name};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Info {
    #[serde(default)]
    formulae: Vec<Formula>,
    #[serde(default)]
    casks: Vec<Cask>,
}

#[derive(Debug, Deserialize)]
struct Formula {
    name: String,
    full_name: Option<String>,
    tap: Option<String>,
    desc: Option<String>,
    homepage: Option<String>,
    #[serde(default)]
    urls: FormulaUrls,
    #[serde(default)]
    installed: Vec<Keg>,
    linked_keg: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FormulaUrls {
    stable: Option<UrlEntry>,
    head: Option<UrlEntry>,
}
#[derive(Debug, Deserialize)]
struct UrlEntry {
    url: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Keg {
    version: String,
}

#[derive(Debug, Deserialize)]
struct Cask {
    token: String,
    full_token: Option<String>,
    tap: Option<String>,
    #[serde(default)]
    name: Vec<String>,
    desc: Option<String>,
    homepage: Option<String>,
    url: Option<String>,
    installed: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PackageMeta {
    pub package: String,
    pub cask: bool,
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub installed: Option<String>,
    urls: Vec<(Kind, String)>,
}

impl PackageMeta {
    pub(crate) fn repositories(&self) -> Vec<(RepoId, MatchEvidence)> {
        self.urls
            .iter()
            .filter_map(|(kind, url)| {
                Some((
                    github_repo_in_url(url)?,
                    MatchEvidence {
                        kind: *kind,
                        value: sanitize_url(url)?,
                    },
                ))
            })
            .collect()
    }
}

pub(crate) fn parse(text: &str) -> Result<Vec<PackageMeta>> {
    let info: Info =
        serde_json::from_str(text).map_err(|error| Error::parse("brew metadata", &error))?;
    let packages: Vec<_> = info
        .formulae
        .into_iter()
        .map(formula)
        .chain(info.casks.into_iter().map(cask))
        .collect();
    for package in &packages {
        if !is_brew_name(&package.package) {
            return Err(Error::InvalidInput(
                "brew metadata contains an invalid package name".into(),
            ));
        }
    }
    Ok(packages)
}

fn full_name(short: &str, full: Option<String>, tap: Option<String>, official: &str) -> String {
    full.unwrap_or_else(|| match tap {
        Some(tap) if tap != official => format!("{tap}/{short}"),
        _ => short.into(),
    })
}

fn formula(formula: Formula) -> PackageMeta {
    let installed = formula
        .linked_keg
        .filter(|linked| formula.installed.iter().any(|keg| &keg.version == linked))
        .or_else(|| match formula.installed.as_slice() {
            [keg] => Some(keg.version.clone()),
            _ => None,
        });
    let urls = [
        (
            Kind::SourceUrl,
            formula.urls.stable.and_then(|entry| entry.url),
        ),
        (Kind::HeadUrl, formula.urls.head.and_then(|entry| entry.url)),
        (Kind::Homepage, formula.homepage.clone()),
    ]
    .into_iter()
    .filter_map(|(kind, url)| url.map(|url| (kind, url)))
    .collect();
    PackageMeta {
        package: full_name(
            &formula.name,
            formula.full_name,
            formula.tap,
            "homebrew/core",
        ),
        cask: false,
        name: formula.name,
        description: formula.desc,
        homepage: formula.homepage,
        installed,
        urls,
    }
}

fn cask(cask: Cask) -> PackageMeta {
    let urls = [
        (Kind::DownloadUrl, cask.url),
        (Kind::Homepage, cask.homepage.clone()),
    ]
    .into_iter()
    .filter_map(|(kind, url)| url.map(|url| (kind, url)))
    .collect();
    PackageMeta {
        package: full_name(&cask.token, cask.full_token, cask.tap, "homebrew/cask"),
        cask: true,
        name: cask.name.into_iter().next().unwrap_or(cask.token),
        description: cask.desc,
        homepage: cask.homepage,
        installed: cask.installed.filter(|version| !version.is_empty()),
        urls,
    }
}
