//! Read-only matching shared by CLI and desktop.
use super::Headway;
use crate::{
    BrewSource, CancellationToken, Error, MatchSide, Result, Source, SourceMatches, SourceSpec,
    TrackedApp,
    brew_match::{evidence, to_brew::github_to_brew, to_github::brew_to_github},
};

impl Headway {
    /// Matches Homebrew packages to GitHub repositories, or repositories to Homebrew packages.
    /// Never installs, tracks or edits anything; optional provider failures remain in `errors`.
    pub async fn match_source(&self, input: &str, offline: bool) -> Result<SourceMatches> {
        let config = self.store.config()?;
        let source = input_source(input, &config.apps)?;
        if matches!(source, SourceSpec::Npm(_) | SourceSpec::Script(_)) {
            return Err(unsupported());
        }
        let (gh, error) = if offline {
            (None, None)
        } else {
            match self.github().await {
                Ok(gh) => (Some(gh), None),
                Err(error) => (None, Some(error)),
            }
        };
        let env = self.env().await;
        let cancel = CancellationToken::new();
        let mut result = match source {
            SourceSpec::Github(repo) => {
                github_to_brew(&cancel, &env, gh.as_ref(), &repo, offline).await?
            }
            SourceSpec::Brew { package, cask } => {
                brew_to_github(
                    &cancel,
                    &env,
                    gh.as_ref(),
                    &BrewSource {
                        package,
                        cask,
                        install_path: None,
                    },
                    offline,
                )
                .await?
            }
            SourceSpec::Npm(_) | SourceSpec::Script(_) => return Err(unsupported()),
        };
        if let Some(error) = error
            && (result.from == MatchSide::Github || result.repositories.is_empty())
        {
            result.error("github", &error);
        }
        evidence::tracked_repositories(&mut result.repositories, &config.apps);
        for package in &mut result.packages {
            package.tracked_by = config
                .apps
                .iter()
                .filter(|app| match &app.source {
                    Source::Brew(source) => {
                        source.cask == package.cask && source.package == package.package
                    }
                    Source::Github(_) | Source::Npm(_) | Source::Script(_) => false,
                })
                .map(|app| app.id.clone())
                .collect();
        }
        Ok(result)
    }
}

fn input_source(input: &str, apps: &[TrackedApp]) -> Result<SourceSpec> {
    match SourceSpec::parse(input) {
        Ok(source) => Ok(source),
        Err(error) => match apps.iter().find(|app| app.id == input) {
            Some(app) => match &app.source {
                Source::Github(source) => Ok(SourceSpec::Github(source.repo_id()?)),
                Source::Brew(source) => Ok(SourceSpec::Brew {
                    package: source.package.clone(),
                    cask: source.cask,
                }),
                Source::Npm(_) | Source::Script(_) => Err(unsupported()),
            },
            None => Err(error),
        },
    }
}

fn unsupported() -> Error {
    Error::InvalidInput("match needs owner/repo, a GitHub URL, brew:<formula>, cask:<cask> or the id of a GitHub/Homebrew app".into())
}
