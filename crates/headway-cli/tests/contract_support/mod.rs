use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

pub fn command(home: &Path) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("headway"));
    cmd.env("HEADWAY_HOME", home)
        .env("HEADWAY_API_URL", "http://127.0.0.1:1")
        .env("HEADWAY_GITHUB_TOKEN", "test-token")
        .env("NO_COLOR", "1")
        .env_remove("HEADWAY_OUTPUT");
    cmd
}

pub fn result(home: &Path, args: &[&str], code: i32) -> Value {
    decode(
        command(home).arg("--json").args(args).output().unwrap(),
        code,
    )
}

pub fn decode(output: Output, code: i32) -> Value {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    let value: Value = serde_json::from_reader(std::io::Cursor::new(output.stdout)).unwrap();
    assert_eq!(value["ok"], code == 0, "{value}");
    if let Some(error) = value.get("error") {
        assert_eq!(error["exit_code"], code);
        assert!(value["next"].is_array());
    }
    value
}

pub fn add(home: &Path, extra: &[&str]) -> Value {
    let mut args = vec!["add", "octo/tool", "--no-inspect", "--build", "true"];
    args.extend_from_slice(extra);
    result(home, &args, 0)
}
