//! Fixtures shared by unit tests.

use std::process::Command;

use crate::config::{AppKind, Install, SourceInstall, Track, TrackedApp};

pub fn cli_app(id: &str) -> TrackedApp {
    TrackedApp {
        id: id.into(),
        name: id.into(),
        repo: format!("owner/{id}"),
        kind: AppKind::Cli,
        auto_update: false,
        git_url: None,
        track: Track::Commit { branch: None },
        install: Install::Source(SourceInstall {
            build: "cargo build --release".into(),
            artifact: Some(format!("target/release/{id}")),
            ..SourceInstall::default()
        }),
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
