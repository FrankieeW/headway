//! Isolated, stateful npm and Homebrew executable fixtures.
use assert_cmd::Command;
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt};

pub(super) struct Sandbox {
    sequence: std::cell::Cell<usize>,
    pub(super) dir: tempfile::TempDir,
}
impl Sandbox {
    pub(super) fn new() -> Self {
        let this = Self {
            sequence: std::cell::Cell::new(0),
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir(this.dir.path().join("bin")).unwrap();
        this.tool("npm", r#"
case "$1" in
view) echo '{"versions":["1.0.0","1.1.0","2.0.0"],"dist-tags":{"latest":"2.0.0","beta":"1.1.0"}}';;
ls) v=$(cat "$HEADWAY_HOME/npm-version"); printf '{"dependencies":{"@acme/tool":{"version":"%s"}}}\n' "$v";;
install) printf '%s' "${3##*@}" > "$HEADWAY_HOME/npm-version"; echo "$*" >> "$HEADWAY_HOME/mutations"; echo installed;;
*) exit 91;;
esac
"#);
        this.tool("brew", r#"
case "$1" in
info)
 v=$(cat "$HEADWAY_HOME/brew-version"); outdated=true; [ "$v" = 2.0.0 ] && outdated=false
 if [ "$3" = --cask ]; then
 printf '{"casks":[{"version":"2.0.0","installed":"%s","outdated":%s}]}\n' "$v" "$outdated"
 else
 printf '{"formulae":[{"versions":{"stable":"2.0.0"},"installed":[{"version":"%s"}],"outdated":%s}]}\n' "$v" "$outdated"
 fi;;
upgrade|install) printf 2.0.0 > "$HEADWAY_HOME/brew-version"; echo "$*" >> "$HEADWAY_HOME/mutations"; echo upgraded;;
*) exit 92;;
esac
"#);
        fs::write(this.dir.path().join("npm-version"), "1.0.0").unwrap();
        fs::write(this.dir.path().join("brew-version"), "1.0.0").unwrap();
        this
    }
    fn tool(&self, name: &str, body: &str) {
        let path = self.dir.path().join("bin").join(name);
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    pub(super) fn command(&self) -> Command {
        let mut cmd = Command::cargo_bin("headway").unwrap();
        cmd.env("HEADWAY_HOME", self.dir.path())
            .env("HEADWAY_API_URL", "http://127.0.0.1:1")
            .env("HEADWAY_GITHUB_TOKEN", "test-token")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.dir.path().join("bin").display()),
            )
            .env("NO_COLOR", "1")
            .env_remove("HEADWAY_OUTPUT");
        cmd
    }
    pub(super) fn run(&self, args: &[&str]) -> Value {
        let output = self.command().arg("--json").args(args).output().unwrap();
        let value: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{args:?}: {e}: {output:?}"));
        assert_eq!(
            output.status.success(),
            value["ok"].as_bool().unwrap(),
            "{value}"
        );
        if let Ok(root) = std::env::var("HEADWAY_TEST_EVIDENCE") {
            fs::create_dir_all(&root).unwrap();
            let name = format!(
                "scenario-{}-{}-{}.json",
                self.dir.path().file_name().unwrap().to_string_lossy(),
                self.sequence.get(),
                args.join("_").replace('/', "_")
            );
            fs::write(std::path::Path::new(&root).join(name), &output.stdout).unwrap();
        }
        self.sequence.set(self.sequence.get() + 1);
        value
    }
    pub(super) fn ok(&self, args: &[&str]) -> Value {
        let value = self.run(args);
        assert_eq!(value["ok"], true, "{value}");
        value
    }
    pub(super) fn unchanged(&self) {
        assert!(!self.dir.path().join("mutations").exists());
    }
}
