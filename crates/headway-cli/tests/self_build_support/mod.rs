use serde::Deserialize;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Stamp {
    #[serde(default)]
    pub repository: Option<String>,
    pub commit: String,
    pub dirty: bool,
    pub target: String,
}

pub struct Repo {
    _temp: tempfile::TempDir,
    pub root: PathBuf,
    bin: PathBuf,
    home: PathBuf,
}

pub fn success(output: Output) -> Output {
    assert!(output.status.success(), "{output:?}");
    output
}

impl Repo {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo with spaces");
        let bin = temp.path().join("bin");
        let home = temp.path().join("home");
        for path in [&root, &bin, &home] {
            fs::create_dir_all(path).unwrap();
        }
        let root = root.canonicalize().unwrap();
        fs::create_dir(root.join("scripts")).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-headway.sh"),
            root.join("scripts/build-headway.sh"),
        )
        .unwrap();
        for script in [
            "macos-build-env.sh",
            "macos-bundle-config.sh",
            "verify-macos-artifacts.sh",
        ] {
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts")
                    .join(script),
                root.join("scripts").join(script),
            )
            .unwrap();
        }
        fs::write(
            root.join(".gitignore"),
            "/target/\n/.headway-out/\n/cache with spaces/\n",
        )
        .unwrap();
        for tool in ["cargo", "rustc", "pnpm", "codesign", "otool", "xcrun"] {
            let path = bin.join(tool);
            fs::write(&path, include_str!("tools.sh")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let repo = Self {
            _temp: temp,
            root,
            bin,
            home,
        };
        repo.git(&["init", "--quiet", "-b", "main"]);
        repo.commit("first");
        repo
    }

    fn command(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.env_clear()
            .current_dir(&self.root)
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
        cmd
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = success(self.command("git").args(args).output().unwrap());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    pub fn commit(&self, content: &str) -> String {
        fs::write(self.root.join("revision"), content).unwrap();
        self.git(&["add", "."]);
        self.git(&["-c", "commit.gpgsign=false", "commit", "-qm", content]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn build(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let output = self
            .command("bash")
            .arg("scripts/build-headway.sh")
            .args(args)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HEADWAY_HOME", &self.home)
            .env("FIXTURE_LOG", self.home.join("tools.log"))
            .env("HEADWAY_GITHUB_TOKEN", "fixture-token-must-not-reach-tools")
            .envs(env.iter().copied())
            .output()
            .unwrap();
        println!(
            "helper {args:?} {env:?}: status={}\nstdout={}stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    pub fn stamp(&self, relative: &str) -> Stamp {
        let output = success(
            self.command(self.root.join(relative).to_str().unwrap())
                .args(["--json", "version"])
                .output()
                .unwrap(),
        );
        println!(
            "artifact {relative}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    pub fn tools(&self) -> String {
        fs::read_to_string(self.home.join("tools.log")).unwrap()
    }

    pub fn headway(&self, home: &Path, api: &str) -> Command {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut cmd = self.command(assert_cmd::cargo::cargo_bin!("headway").to_str().unwrap());
        cmd.arg("--json")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HEADWAY_HOME", home)
            .env("HEADWAY_API_URL", api)
            .env("FIXTURE_LOG", self.home.join("tools.log"))
            .env("HEADWAY_GITHUB_TOKEN", "fixture-token-must-not-reach-tools");
        cmd
    }
}
