use std::collections::BTreeMap;

use serde::Deserialize;

use super::{InventoryCandidate, candidate};
use crate::{BrewSource, Error, NpmSource, Result, Source};

#[derive(Deserialize)]
struct BrewInfo {
    formulae: Vec<Formula>,
    casks: Vec<Cask>,
}

#[derive(Deserialize)]
struct Formula {
    name: String,
    full_name: Option<String>,
    linked_keg: Option<String>,
    installed: Vec<Keg>,
}

#[derive(Deserialize)]
struct Keg {
    version: String,
}

#[derive(Deserialize)]
struct Cask {
    token: String,
    full_token: Option<String>,
    tap: Option<String>,
    #[serde(default)]
    name: Vec<String>,
    installed: Option<String>,
    #[serde(default)]
    artifacts: Vec<super::cask_path::Artifact>,
}

pub(super) fn brew(text: &str) -> Result<Vec<InventoryCandidate>> {
    let metadata = crate::brew_match::metadata::parse(text)?;
    let info: BrewInfo =
        serde_json::from_str(text).map_err(|err| Error::parse("brew inventory", &err))?;
    let mut candidates = Vec::new();
    for formula in info.formulae {
        if formula.installed.is_empty() {
            continue;
        }
        let version = formula
            .linked_keg
            .filter(|linked| formula.installed.iter().any(|keg| &keg.version == linked))
            .or_else(|| {
                (formula.installed.len() == 1).then(|| formula.installed[0].version.clone())
            });
        let mut item = candidate(
            Source::Brew(BrewSource {
                package: formula.full_name.unwrap_or_else(|| formula.name.clone()),
                cask: false,
                install_path: None,
            }),
            formula.name,
            version,
        );
        if formula.installed.len() > 1 {
            item.conflicts.push(format!(
                "multiple installed versions: {}",
                formula
                    .installed
                    .iter()
                    .map(|keg| keg.version.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        item.app.validate()?;
        candidates.push(item);
    }
    for cask in info.casks {
        let Some(version) = cask.installed.filter(|version| !version.is_empty()) else {
            continue;
        };
        let install_path = super::cask_path::locate(&cask.token, &cask.artifacts);
        let package = cask
            .full_token
            .unwrap_or_else(|| match cask.tap.as_deref() {
                Some(tap) if tap != "homebrew/cask" => format!("{tap}/{}", cask.token),
                _ => cask.token.clone(),
            });
        let item = candidate(
            Source::Brew(BrewSource {
                package,
                cask: true,
                install_path,
            }),
            cask.name.first().cloned().unwrap_or(cask.token),
            Some(version),
        );
        item.app.validate()?;
        candidates.push(item);
    }
    for item in &mut candidates {
        if let Source::Brew(source) = &item.app.source
            && let Some(package) = metadata
                .iter()
                .find(|package| package.package == source.package && package.cask == source.cask)
        {
            item.repositories = crate::brew_match::evidence::repositories(package.repositories());
        }
    }
    Ok(candidates)
}

#[derive(Deserialize)]
struct NpmList {
    #[serde(default)]
    dependencies: BTreeMap<String, NpmPackage>,
}

#[derive(Deserialize)]
struct NpmPackage {
    version: Option<String>,
    #[serde(default)]
    missing: bool,
    #[serde(default)]
    problems: Vec<String>,
}

pub(super) fn npm(text: &str) -> Result<Vec<InventoryCandidate>> {
    let list: NpmList =
        serde_json::from_str(text).map_err(|err| Error::parse("npm inventory", &err))?;
    list.dependencies
        .into_iter()
        .filter(|(_, package)| !package.missing)
        .map(|(name, package)| {
            let mut item = candidate(
                Source::Npm(NpmSource {
                    package: name.clone(),
                    dist_tag: "latest".into(),
                }),
                name,
                package.version,
            );
            item.conflicts = package.problems;
            item.app.validate()?;
            Ok(item)
        })
        .collect()
}
