use super::{BuildCandidate, BuildCandidateSource};
use crate::config::AppKind;

/// Conservative eligibility filter, not a shell sandbox. Suggestions still need review.
pub(super) fn safe(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.replace("&&", "").contains('&') {
        return false;
    }
    if [
        "$",
        "`",
        ";",
        "|",
        ">",
        "<",
        "\0",
        "#",
        "--global",
        " -g",
        "--watch",
        "windows",
        "linux",
        "powershell",
        "cmd.exe",
        "darwin=false",
    ]
    .iter()
    .any(|word| lower.contains(word))
    {
        return false;
    }
    !lower
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .any(|word| {
            matches!(
                word,
                "sudo"
                    | "rm"
                    | "rmdir"
                    | "curl"
                    | "wget"
                    | "eval"
                    | "exec"
                    | "chmod"
                    | "chown"
                    | "launchctl"
                    | "systemctl"
                    | "shutdown"
                    | "reboot"
                    | "test"
                    | "tests"
                    | "dev"
                    | "serve"
                    | "start"
                    | "apt"
                    | "brew"
            )
        })
}

pub(super) fn candidates(file: &str, text: &str, source: &[BuildCandidate]) -> Vec<BuildCandidate> {
    let mut out = Vec::new();
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut fence: Option<&str> = None;
    let mut eligible = false;
    let mut start = 0;
    let mut lines = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if let Some(marker) = fence {
            if line.starts_with(marker) {
                if eligible && let Some(candidate) = block(file, start, &lines, source) {
                    out.push(candidate);
                }
                fence = None;
                lines.clear();
            } else {
                lines.push(line.strip_prefix("$ ").unwrap_or(line));
            }
            continue;
        }
        if line.starts_with('#') {
            let depth = line.bytes().take_while(|byte| *byte == b'#').count();
            headings.retain(|(parent, _)| *parent < depth);
            headings.push((depth, line.to_ascii_lowercase()));
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            let marker = &line[..3];
            let language = line[3..].trim();
            let context = headings
                .iter()
                .map(|(_, heading)| heading.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            eligible = matches!(language, "" | "sh" | "bash" | "shell" | "console" | "zsh")
                && ["build", "development", "source", "compil"]
                    .iter()
                    .any(|word| context.contains(word))
                && !["windows", "linux", "test", "uninstall"]
                    .iter()
                    .any(|word| context.contains(word));
            start = u32::try_from(index + 2).unwrap_or(u32::MAX);
            fence = Some(marker);
        }
    }
    out
}

fn block(
    file: &str,
    line: u32,
    lines: &[&str],
    source: &[BuildCandidate],
) -> Option<BuildCandidate> {
    let text = lines
        .iter()
        .copied()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.replace("\\\n", " ");
    if text.is_empty() || !safe(&text) {
        return None;
    }
    let commands: Vec<&str> = text.split('\n').collect();
    if !commands
        .iter()
        .flat_map(|line| line.split("&&"))
        .all(|command| {
            let command = command.trim();
            [
                "cargo ",
                "pnpm ",
                "npm ",
                "yarn ",
                "bun ",
                "go build",
                "swift build",
                "make",
                "cmake ",
                "xcodebuild ",
                "./",
                "cd ",
                "git submodule update ",
            ]
            .iter()
            .any(|prefix| command.starts_with(prefix))
                && !command.starts_with("cd /")
                && !command.contains("../")
        })
    {
        return None;
    }
    let build = commands.join(" && ");
    if !["build", "make", "compile", "bundle"]
        .iter()
        .any(|word| build.contains(word))
    {
        return None;
    }
    let corroborated = source.iter().find(|candidate| {
        candidate.build.rsplit(" && ").next().is_some_and(|step| {
            build.contains(step) || build.contains(&step.replace(" --locked", ""))
        })
    });
    // Only byte-identical commands in the same working directory inherit proven output.
    let artifact = source
        .iter()
        .find(|candidate| candidate.build == build && candidate.workdir.is_none())
        .and_then(|candidate| candidate.artifact.clone());
    let reason = if artifact.is_some() {
        "README sequence exactly matches a manifest command with a known output."
    } else {
        "README build section; review this editable sequence and set its output path."
    };
    Some(BuildCandidate {
        id: format!("readme:{file}:{line}"),
        source: BuildCandidateSource::Readme,
        file: file.into(),
        line: Some(line),
        build,
        workdir: None,
        artifact,
        kind: corroborated.map_or(AppKind::Cli, |candidate| candidate.kind),
        reason: reason.into(),
        recommended: corroborated.is_some(),
    })
}
