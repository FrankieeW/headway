//! Explicit, per-build tool versions. No project or global settings are written.
use crate::process::Cmd;
use crate::shell::ShellEnv;
use std::collections::BTreeSet;

/// Validate portable, exact mise tool requests without accepting backend URLs or shell syntax.
pub fn validate(tools: &[String]) -> std::result::Result<(), String> {
    if tools.len() > 16 {
        return Err("install.tools supports at most 16 entries".into());
    }
    let mut names = BTreeSet::new();
    for tool in tools {
        let Some((name, version)) = tool.split_once('@') else {
            return Err(format!("invalid tool `{tool}`: use name@version"));
        };
        let numeric = version.split(['-', '+']).next().unwrap_or_default();
        let parts: Vec<&str> = numeric.split('.').collect();
        if name.is_empty()
            || name.len() > 64
            || !name.starts_with(|c: char| c.is_ascii_alphabetic())
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || version.is_empty()
            || version.len() > 64
            || parts.len() != 3
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
            || !version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-+_".contains(c))
            || !names.insert(name.to_ascii_lowercase())
        {
            return Err(format!(
                "invalid or duplicate tool `{tool}`: use name@version"
            ));
        }
    }
    Ok(())
}

/// Build through mise when explicit versions are configured; the shell command stays an argv value.
pub fn build_command(env: &ShellEnv, build: &str, tools: &[String]) -> Cmd {
    if tools.is_empty() {
        return Cmd::new("/bin/sh", env).args(["-c", build]);
    }
    let mut cmd = Cmd::new("mise", env).args(["--yes", "exec"]);
    for tool in tools {
        cmd = cmd.arg(tool);
    }
    let script = if tools.iter().any(|tool| tool.starts_with("zig@")) {
        "export ZIG=\"$(command -v zig)\"; exec /bin/sh -c \"$1\""
    } else {
        "exec /bin/sh -c \"$1\""
    };
    cmd.args(["--", "/bin/sh", "-c", script, "headway", build])
}

pub(crate) fn add_plan_step(steps: &mut Vec<String>, tools: &[String]) {
    if !tools.is_empty() {
        steps.insert(
            2,
            format!(
                "use project tools via mise: {} (missing versions may be installed)",
                tools.join(", ")
            ),
        );
    }
}

#[cfg(test)]
#[path = "toolchain_tests.rs"]
mod tests;
