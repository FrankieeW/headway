use super::{Headway, TokenStatus, join_error};
use crate::config::TrackedApp;
use crate::error::{Error, ErrorInfo, Result};
use crate::github::GitHub;
use crate::inspect::{self, RepoInspection};
use crate::source::SourceSpec;
use crate::token::{self, ResolvedToken, TokenSource};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

impl Headway {
    // ---- GitHub ------------------------------------------------------------

    pub async fn resolve_token(&self) -> Result<ResolvedToken> {
        let gh_fallback = self.store.config()?.settings.gh_fallback;
        let env = self.env().await;
        let secrets = Arc::clone(&self.secrets);
        // `security` and `gh` are subprocesses: keep them off the async workers.
        tokio::task::spawn_blocking(move || token::resolve(secrets.as_ref(), &env, gh_fallback))
            .await
            .map_err(join_error)
    }

    pub async fn github(&self) -> Result<GitHub> {
        GitHub::new(&self.api, self.resolve_token().await?.token)
    }

    pub async fn token_status(&self) -> Result<TokenStatus> {
        let resolved = self.resolve_token().await?;
        let gh = GitHub::new(&self.api, resolved.token.clone())?;
        let Some(secret) = resolved.token else {
            return Ok(TokenStatus {
                source: TokenSource::None,
                masked: None,
                login: None,
                rate: gh.rate_limit().await.ok(),
                error: None,
            });
        };
        let (user, rate) = tokio::join!(gh.user(), gh.rate_limit());
        let (login, error) = match user {
            Ok(user) => (Some(user.login), None),
            Err(err) => (None, Some(ErrorInfo::from(&err))),
        };
        Ok(TokenStatus {
            source: resolved.source,
            masked: Some(token::mask(&secret)),
            login,
            rate: rate.ok(),
            error,
        })
    }

    /// Stores a token in the keychain, after checking that GitHub accepts it.
    pub async fn set_token(&self, token: &str, verify: bool) -> Result<TokenStatus> {
        let token = token::validate_token(token)?;
        if verify {
            GitHub::new(&self.api, Some(token.clone()))?.user().await?;
        }
        let secrets = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || secrets.set(&token))
            .await
            .map_err(join_error)??;
        self.token_status().await
    }

    pub async fn clear_token(&self) -> Result<bool> {
        let secrets = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || secrets.delete())
            .await
            .map_err(join_error)?
    }

    pub async fn inspect(&self, repo: &str) -> Result<RepoInspection> {
        inspect::inspect(&self.github().await?, repo).await
    }

    /// Inspects an npm package or Homebrew formula/cask without using GitHub credentials.
    pub async fn lookup(&self, input: &str) -> Result<inspect::PackageLookup> {
        let env = self.env().await;
        match SourceSpec::parse(input)? {
            SourceSpec::Npm(package) => {
                crate::npm::lookup(&CancellationToken::new(), &env, &package).await
            }
            SourceSpec::Brew { package, cask } => {
                crate::brew::lookup(&CancellationToken::new(), &env, &package, cask).await
            }
            SourceSpec::Github(_) | SourceSpec::Script(_) => Err(Error::InvalidInput(
                "lookup needs npm:<package>, brew:<formula> or cask:<cask>".into(),
            )),
        }
    }

    pub(super) async fn github_for(&self, app: &TrackedApp) -> Result<GitHub> {
        if app.github().is_some() {
            self.github().await
        } else {
            GitHub::new(&self.api, None)
        }
    }
}
