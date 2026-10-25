//! Git operations for source installs: clone once, then fetch and check out the
//! exact ref to build. The checkout is reused so build caches (`target/`, …) stay warm.

use std::path::Path;

use base64::Engine as _;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::events::LogStream;
use crate::process::{self, Cmd};
use crate::shell::ShellEnv;

mod retry;

#[cfg(test)]
#[path = "git/retry_tests.rs"]
mod retry_tests;

/// Environment for git: never prompt, and authenticate to github.com with the token
/// through an `http.extraheader` passed in env vars (not in URLs or argv).
pub fn git_env(env: &ShellEnv, url: &str, token: Option<&str>) -> ShellEnv {
    let env = env.clone().with_var("GIT_TERMINAL_PROMPT", "0");
    match token {
        Some(token) if url.starts_with("https://github.com/") => {
            let basic =
                base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
            env.with_var("GIT_CONFIG_COUNT", "1")
                .with_var("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
                .with_var(
                    "GIT_CONFIG_VALUE_0",
                    format!("AUTHORIZATION: basic {basic}"),
                )
        }
        _ => env,
    }
}

pub struct Checkout<'a> {
    pub dir: &'a Path,
    pub env: &'a ShellEnv,
    pub cancel: &'a CancellationToken,
}

impl Checkout<'_> {
    fn git(&self) -> Cmd {
        Cmd::new("git", self.env)
            .arg("-C")
            .arg(self.dir.display().to_string())
    }

    /// Clones `url` (blobs on demand) if needed, then fetches all branches and tags.
    pub async fn sync(
        &self,
        url: &str,
        on_line: &mut (dyn FnMut(LogStream, &str) + Send),
    ) -> Result<()> {
        if self.dir.join(".git").is_dir() {
            let set_url = self.git().args(["remote", "set-url", "origin", url]);
            process::run(&set_url, self.cancel, on_line).await?;
        } else {
            if let Some(parent) = self.dir.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| Error::io_at("create", parent, err))?;
            }
            let clone = Cmd::new("git", self.env).args([
                "clone",
                "--filter=blob:none",
                "--no-checkout",
                url,
                &self.dir.display().to_string(),
            ]);
            retry::clone(&clone, self.dir, self.cancel, on_line).await?;
        }
        let fetch = self.git().args([
            "fetch",
            "--force",
            "--prune",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
            "+refs/tags/*:refs/tags/*",
        ]);
        retry::run(&fetch, self.cancel, on_line).await
    }

    /// Checks out `reference` (SHA or `refs/tags/…`) detached and returns the commit SHA.
    pub async fn checkout(
        &self,
        reference: &str,
        on_line: &mut (dyn FnMut(LogStream, &str) + Send),
    ) -> Result<String> {
        if reference.is_empty()
            || reference.starts_with('-')
            || reference.chars().any(char::is_control)
        {
            return Err(Error::InvalidInput("invalid git checkout reference".into()));
        }
        if !self.has_commit(reference).await {
            // Commits only reachable by SHA (e.g. from a pull request) need an explicit fetch.
            let fetch = self.git().args(["fetch", "origin", reference]);
            retry::run(&fetch, self.cancel, on_line).await?;
        }
        let checkout = self
            .git()
            .args(["checkout", "--force", "--detach", reference]);
        // A blobless clone downloads missing objects during checkout as well as fetch.
        retry::run(&checkout, self.cancel, on_line).await?;
        // Remove stray untracked files but keep ignored ones: those are the build caches.
        // Headway's own output dirs (Xcode derived data, Go output) are build caches too.
        let clean = self.git().args([
            "clean",
            "-ffd",
            "-e",
            "/.headway-build",
            "-e",
            "/.headway-out",
        ]);
        process::run(&clean, self.cancel, on_line).await?;
        if self.dir.join(".gitmodules").exists() {
            let submodules =
                self.git()
                    .args(["submodule", "update", "--init", "--recursive", "--force"]);
            retry::run(&submodules, self.cancel, on_line).await?;
        }
        self.rev_parse("HEAD").await
    }

    /// `git describe --tags --always` for display, if it works.
    pub async fn describe(&self) -> Option<String> {
        let out = process::capture(&self.git().args(["describe", "--tags", "--always"]))
            .await
            .ok()?;
        let described = out.stdout.trim().to_owned();
        (out.success() && !described.is_empty()).then_some(described)
    }

    async fn has_commit(&self, reference: &str) -> bool {
        let verify = self.git().args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ]);
        process::capture(&verify)
            .await
            .is_ok_and(|out| out.success())
    }

    async fn rev_parse(&self, reference: &str) -> Result<String> {
        let cmd = self.git().args(["rev-parse", reference]);
        let out = process::capture(&cmd).await?;
        if out.success() {
            Ok(out.stdout.trim().to_owned())
        } else {
            Err(Error::CommandFailed {
                command: cmd.display(),
                status: out.status,
                tail: out.stderr.lines().map(str::to_owned).collect(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::GitFixture;

    #[tokio::test]
    async fn changing_origins_drops_foreign_tags_without_discarding_build_cache() {
        // Given an owned checkout whose previous origin published a tag.
        let first = GitFixture::new();
        first.commit("payload", "first", "first origin");
        first.tag("v9.0.0");
        let second = GitFixture::new();
        second.commit("payload", "second", "second origin");
        second.tag("v1.0.0");
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("checkout");
        let env = ShellEnv::current();
        let cancel = CancellationToken::new();
        let checkout = Checkout {
            dir: &dir,
            env: &env,
            cancel: &cancel,
        };
        checkout.sync(&first.url(), &mut |_, _| {}).await.unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/cache"), "cached build").unwrap();
        // When switching the same checkout to another origin.
        checkout.sync(&second.url(), &mut |_, _| {}).await.unwrap();
        // Then describe cannot use the old origin's tag, and ignored build caches survive.
        let tags = process::capture(&checkout.git().args(["tag", "--list"]))
            .await
            .unwrap();
        assert!(tags.success());
        assert_eq!(tags.stdout.trim(), "v1.0.0");
        assert_eq!(
            std::fs::read_to_string(dir.join("target/cache")).unwrap(),
            "cached build"
        );
    }

    #[test]
    fn token_only_goes_to_github_https() {
        let env = ShellEnv::from_vars([]);
        let with = git_env(&env, "https://github.com/o/r.git", Some("ghp_secret"));
        assert_eq!(
            with.get("GIT_CONFIG_KEY_0"),
            Some("http.https://github.com/.extraheader")
        );
        assert!(
            with.get("GIT_CONFIG_VALUE_0")
                .unwrap()
                .starts_with("AUTHORIZATION: basic ")
        );
        assert_eq!(with.get("GIT_TERMINAL_PROMPT"), Some("0"));

        let ssh = git_env(&env, "git@github.com:o/r.git", Some("ghp_secret"));
        assert!(ssh.get("GIT_CONFIG_COUNT").is_none());
        let elsewhere = git_env(&env, "https://gitlab.com/o/r.git", Some("ghp_secret"));
        assert!(elsewhere.get("GIT_CONFIG_COUNT").is_none());
    }

    #[tokio::test]
    async fn clones_fetches_and_checks_out_commits_and_tags() {
        let fixture = GitFixture::new();
        let first = fixture.commit("a.txt", "one", "first");
        fixture.tag("v0.1.0");
        let second = fixture.commit("a.txt", "two", "second");

        let work = tempfile::tempdir().unwrap();
        let dir = work.path().join("checkout");
        let env = ShellEnv::current();
        let cancel = CancellationToken::new();
        let checkout = Checkout {
            dir: &dir,
            env: &env,
            cancel: &cancel,
        };

        checkout.sync(&fixture.url(), &mut |_, _| {}).await.unwrap();
        assert_eq!(
            checkout.checkout(&second, &mut |_, _| {}).await.unwrap(),
            second
        );
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "two");

        // Stray files are cleaned, ignored build output survives.
        std::fs::write(dir.join("stray.txt"), "x").unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/cache"), "warm").unwrap();
        assert_eq!(
            checkout
                .checkout("refs/tags/v0.1.0", &mut |_, _| {})
                .await
                .unwrap(),
            first
        );
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one");
        assert!(!dir.join("stray.txt").exists());
        assert!(dir.join("target/cache").exists());
        assert_eq!(checkout.describe().await.as_deref(), Some("v0.1.0"));

        // A second sync reuses the clone and sees new commits.
        let third = fixture.commit("a.txt", "three", "third");
        checkout.sync(&fixture.url(), &mut |_, _| {}).await.unwrap();
        assert_eq!(
            checkout.checkout(&third, &mut |_, _| {}).await.unwrap(),
            third
        );
    }
}
