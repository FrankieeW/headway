//! GitHub token storage and resolution.
//!
//! The token lives in the macOS login keychain. All access goes through
//! `/usr/bin/security` (the approach `gh` uses): the keychain item then trusts that
//! one tool, so neither the CLI nor the app ever triggers a keychain prompt — which
//! matters when an agent drives the CLI. The token is passed on stdin, never argv.
//!
//! Resolution order: `HEADWAY_GITHUB_TOKEN` → keychain → `GITHUB_TOKEN` →
//! `GH_TOKEN` → `gh auth token` (if enabled).

use std::fmt;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::shell::ShellEnv;

pub const KEYCHAIN_SERVICE: &str = "wang.frankie.headway";
pub const KEYCHAIN_ACCOUNT: &str = "github-token";
pub const TOKEN_ENV: &str = "HEADWAY_GITHUB_TOKEN";
const SECURITY: &str = "/usr/bin/security";
/// `security` exits with this status when the item does not exist.
const ITEM_NOT_FOUND: i32 = 44;

pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>>;
    fn set(&self, token: &str) -> Result<()>;
    /// Returns whether a token was removed.
    fn delete(&self) -> Result<bool>;
}

/// The login keychain, via `/usr/bin/security`.
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn get(&self) -> Result<Option<String>> {
        let output = Command::new(SECURITY)
            .args([
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
                "-w",
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|err| Error::Keychain(format!("run {SECURITY}: {err}")))?;
        match output.status.code() {
            Some(0) => {
                let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                Ok((!token.is_empty()).then_some(token))
            }
            Some(ITEM_NOT_FOUND) => Ok(None),
            _ => Err(Error::Keychain(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            )),
        }
    }

    fn set(&self, token: &str) -> Result<()> {
        let token = validate_token(token)?;
        let mut child = Command::new(SECURITY)
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| Error::Keychain(format!("run {SECURITY}: {err}")))?;
        // Token characters are validated to [A-Za-z0-9_], so the line cannot be broken out of.
        let line = format!(
            "add-generic-password -U -s {KEYCHAIN_SERVICE} -a {KEYCHAIN_ACCOUNT} -l \"Headway GitHub token\" -w {token}\n"
        );
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(line.as_bytes())
                .map_err(|err| Error::Keychain(format!("write to {SECURITY}: {err}")))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|err| Error::Keychain(format!("wait for {SECURITY}: {err}")))?;
        // `security -i` reports command errors on stderr but may still exit 0: read back.
        match self.get()? {
            Some(stored) if stored == token => Ok(()),
            _ => Err(Error::Keychain(format!(
                "the token was not stored: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }

    fn delete(&self) -> Result<bool> {
        let output = Command::new(SECURITY)
            .args([
                "delete-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .output()
            .map_err(|err| Error::Keychain(format!("run {SECURITY}: {err}")))?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(ITEM_NOT_FOUND) => Ok(false),
            _ => Err(Error::Keychain(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            )),
        }
    }
}

/// In-memory store for tests and sandboxes.
#[derive(Default)]
pub struct MemoryStore {
    token: Mutex<Option<String>>,
}

impl MemoryStore {
    pub fn with_token(token: &str) -> Self {
        Self {
            token: Mutex::new(Some(token.to_owned())),
        }
    }
}

impl SecretStore for MemoryStore {
    fn get(&self) -> Result<Option<String>> {
        Ok(self.token.lock().map_err(poisoned)?.clone())
    }

    fn set(&self, token: &str) -> Result<()> {
        *self.token.lock().map_err(poisoned)? = Some(validate_token(token)?);
        Ok(())
    }

    fn delete(&self) -> Result<bool> {
        Ok(self.token.lock().map_err(poisoned)?.take().is_some())
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> Error {
    Error::Keychain("secret store lock poisoned".into())
}

/// Trims and checks a token: GitHub tokens are 20–255 characters of `[A-Za-z0-9_]`.
pub fn validate_token(token: &str) -> Result<String> {
    let token = token.trim();
    let well_formed = (20..=255).contains(&token.len())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if well_formed {
        Ok(token.to_owned())
    } else {
        Err(Error::InvalidInput(
            "that does not look like a GitHub token (expected 20-255 characters of A-Z, a-z, 0-9, _)"
                .into(),
        ))
    }
}

/// `ghp_…abcd` — enough to recognise a token without revealing it.
pub fn mask(token: &str) -> String {
    let prefix: String = token.chars().take_while(|c| *c != '_').collect();
    let has_prefix = prefix.len() < token.len() && prefix.len() <= 10;
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if has_prefix {
        format!("{prefix}_…{tail}")
    } else {
        format!("…{tail}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    /// `HEADWAY_GITHUB_TOKEN`
    HeadwayEnv,
    Keychain,
    /// `GITHUB_TOKEN`
    GithubEnv,
    /// `GH_TOKEN`
    GhEnv,
    /// `gh auth token`
    GhCli,
    None,
}

#[derive(Clone)]
pub struct ResolvedToken {
    pub token: Option<String>,
    pub source: TokenSource,
}

impl fmt::Debug for ResolvedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedToken")
            .field("token", &self.token.as_deref().map(mask))
            .field("source", &self.source)
            .finish()
    }
}

/// Finds the token to use. A keychain failure does not block the fallbacks.
pub fn resolve(store: &dyn SecretStore, env: &ShellEnv, gh_fallback: bool) -> ResolvedToken {
    let found = |token: String, source| ResolvedToken {
        token: Some(token),
        source,
    };
    let from_env = |key: &str| {
        env.get(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };

    if let Some(token) = from_env(TOKEN_ENV) {
        return found(token, TokenSource::HeadwayEnv);
    }
    if let Ok(Some(token)) = store.get() {
        return found(token, TokenSource::Keychain);
    }
    if let Some(token) = from_env("GITHUB_TOKEN") {
        return found(token, TokenSource::GithubEnv);
    }
    if let Some(token) = from_env("GH_TOKEN") {
        return found(token, TokenSource::GhEnv);
    }
    if gh_fallback && let Some(token) = gh_cli_token(env) {
        return found(token, TokenSource::GhCli);
    }
    ResolvedToken {
        token: None,
        source: TokenSource::None,
    }
}

/// `gh auth token`, if the GitHub CLI is installed and logged in.
pub fn gh_cli_token(env: &ShellEnv) -> Option<String> {
    let gh = env.which("gh")?;
    let output = Command::new(gh)
        .args(["auth", "token", "--hostname", "github.com"])
        .env_clear()
        .envs(env.vars())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

    fn env(vars: &[(&str, &str)]) -> ShellEnv {
        ShellEnv::from_vars(
            vars.iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .chain([("PATH".to_owned(), "/nonexistent".to_owned())]),
        )
    }

    #[test]
    fn validates_token_shape() {
        assert_eq!(validate_token(&format!("  {TOKEN}\n")).unwrap(), TOKEN);
        assert!(validate_token("short").is_err());
        assert!(validate_token("ghp_has space in it 123456789").is_err());
        assert!(validate_token("ghp_injection\nquit\nxxxxxxxxxx").is_err());
    }

    #[test]
    fn masks_tokens() {
        assert_eq!(mask(TOKEN), "ghp_…6789");
        assert_eq!(mask("github_pat_11ABCDEFG_xyz1234"), "github_…1234");
        assert_eq!(mask("0123456789abcdef0123456789"), "…6789");
    }

    #[test]
    fn resolution_order() {
        let store = MemoryStore::with_token("ghp_fromkeychainxxxxxxxxxxxxxxxxxxxxx");
        let all = env(&[
            (TOKEN_ENV, "ghp_headway"),
            ("GITHUB_TOKEN", "ghp_github"),
            ("GH_TOKEN", "ghp_gh"),
        ]);
        assert_eq!(resolve(&store, &all, false).source, TokenSource::HeadwayEnv);

        let generic = env(&[("GITHUB_TOKEN", "ghp_github"), ("GH_TOKEN", "ghp_gh")]);
        assert_eq!(
            resolve(&store, &generic, false).source,
            TokenSource::Keychain
        );

        let empty_store = MemoryStore::default();
        let resolved = resolve(&empty_store, &generic, false);
        assert_eq!(resolved.source, TokenSource::GithubEnv);
        assert_eq!(resolved.token.as_deref(), Some("ghp_github"));

        let gh_only = env(&[("GH_TOKEN", "ghp_gh"), ("GITHUB_TOKEN", "  ")]);
        assert_eq!(
            resolve(&empty_store, &gh_only, false).source,
            TokenSource::GhEnv
        );

        // gh fallback off: PATH always includes Homebrew's bin, where a real `gh` may live.
        let none = resolve(&empty_store, &env(&[]), false);
        assert_eq!(none.source, TokenSource::None);
        assert!(none.token.is_none());
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::default();
        assert_eq!(store.get().unwrap(), None);
        store.set(TOKEN).unwrap();
        assert_eq!(store.get().unwrap().as_deref(), Some(TOKEN));
        assert!(store.delete().unwrap());
        assert!(!store.delete().unwrap());
        assert!(store.set("bad").is_err());
    }

    #[test]
    fn debug_output_is_redacted() {
        let resolved = ResolvedToken {
            token: Some(TOKEN.into()),
            source: TokenSource::Keychain,
        };
        assert!(!format!("{resolved:?}").contains(TOKEN));
    }
}
