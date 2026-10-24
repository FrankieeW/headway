//! Fixtures shared by unit tests.

use std::process::Command;

use crate::config::{AppKind, Install, SourceInstall, Track, TrackedApp};
use crate::source::{GithubSource, Source};

/// Executable fixtures scoped to a per-test PATH, without changing process env.
pub struct FakeTools {
    dir: tempfile::TempDir,
}

impl FakeTools {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn tool(self, name: &str, body: &str) -> Self {
        use std::os::unix::fs::PermissionsExt as _;
        let path = self.dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        self
    }

    pub fn env(&self) -> crate::shell::ShellEnv {
        crate::shell::ShellEnv::from_vars([(
            "PATH".to_owned(),
            format!("{}:/usr/bin:/bin", self.dir.path().display()),
        )])
    }
}

pub fn cli_app(id: &str) -> TrackedApp {
    TrackedApp {
        id: id.into(),
        name: id.into(),
        kind: AppKind::Cli,
        auto_update: false,
        update_policy: crate::UpdatePolicy::default(),
        source: Source::Github(GithubSource {
            repo: format!("owner/{id}"),
            git_url: None,
            track: Track::Commit { branch: None },
            install: Install::Source(SourceInstall {
                build: "cargo build --release".into(),
                artifact: Some(format!("target/release/{id}")),
                ..SourceInstall::default()
            }),
        }),
    }
}

/// `app` with its GitHub source changed by `f`.
pub fn with_github(app: TrackedApp, f: impl FnOnce(GithubSource) -> GithubSource) -> TrackedApp {
    match app.source {
        Source::Github(github) => TrackedApp {
            source: Source::Github(f(github)),
            ..app
        },
        source => TrackedApp { source, ..app },
    }
}

/// A throwaway upstream repository, isolated from the user's git config.
pub struct GitFixture {
    dir: tempfile::TempDir,
}

impl GitFixture {
    /// A repo on `main` whose first commit ignores `target/`.
    pub fn new() -> Self {
        let fixture = Self {
            dir: tempfile::tempdir().expect("tempdir"),
        };
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.commit(".gitignore", "target/\n", "initial");
        fixture
    }

    pub fn url(&self) -> String {
        format!("file://{}", self.dir.path().display())
    }

    pub fn write(&self, file: &str, contents: &str) {
        let path = self.dir.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create dirs");
        }
        std::fs::write(path, contents).expect("write file");
    }

    /// Writes `file`, commits everything and returns the new HEAD SHA.
    pub fn commit(&self, file: &str, contents: &str, message: &str) -> String {
        self.write(file, contents);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.head()
    }

    pub fn tag(&self, name: &str) {
        self.git(&["tag", name]);
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=Headway Test",
                "-c",
                "user.email=test@headway.invalid",
            ])
            .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}
