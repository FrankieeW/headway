//! Source-specific flags and their applicability.
use crate::{args::AppFields, fields::optional};
use headway_core::{Error, Result, Source};

pub fn validate(source: &Source, f: &AppFields) -> Result<()> {
    let github = matches!(source, Source::Github(_));
    let npm = matches!(source, Source::Npm(_));
    let brew = matches!(source, Source::Brew(_));
    let script = matches!(source, Source::Script(_));
    for (flag, present, allowed, needs) in [
        (
            "--keep-source",
            f.keep_source.is_some(),
            github,
            "GitHub source builds",
        ),
        (
            "--source-root",
            f.source_root.is_some(),
            github,
            "GitHub source builds",
        ),
        (
            "--verify-command",
            f.verify_command.is_some(),
            github,
            "GitHub",
        ),
        ("--track", f.track.is_some(), github, "GitHub"),
        ("--branch", f.branch.is_some(), github, "GitHub"),
        ("--prerelease", f.prerelease.is_some(), github, "GitHub"),
        ("--tag-filter", f.tag_filter.is_some(), github, "GitHub"),
        ("--method", f.method.is_some(), github, "GitHub"),
        ("--preset", f.preset.is_some(), github, "GitHub"),
        ("--build", f.build.is_some(), github, "GitHub"),
        ("--workdir", f.workdir.is_some(), github, "GitHub"),
        ("--artifact", f.artifact.is_some(), github, "GitHub"),
        ("--dest", f.dest.is_some(), github, "GitHub"),
        ("--pattern", f.pattern.is_some(), github, "GitHub"),
        ("--binary", f.binary.is_some(), github, "GitHub"),
        ("--git-url", f.git_url.is_some(), github, "GitHub"),
        ("--dist-tag", f.dist_tag.is_some(), npm, "npm"),
        ("--cask", f.cask.is_some(), brew, "Homebrew"),
        (
            "--package",
            f.package.is_some(),
            npm || brew,
            "npm/Homebrew",
        ),
        (
            "--update-command",
            f.update_command.is_some(),
            script,
            "script",
        ),
        (
            "--version-command",
            f.version_command.is_some(),
            script,
            "script",
        ),
        (
            "--latest-command",
            f.latest_command.is_some(),
            script,
            "script",
        ),
    ] {
        if present && !allowed {
            return Err(Error::InvalidInput(format!(
                "{flag} only applies to {needs} sources"
            )));
        }
    }
    Ok(())
}

pub fn apply(source: Source, f: &AppFields) -> Source {
    match source {
        Source::Github(github) => Source::Github(github),
        Source::Npm(mut npm) => {
            if let Some(package) = &f.package {
                npm.package.clone_from(package);
            }
            if let Some(tag) = &f.dist_tag {
                npm.dist_tag.clone_from(tag);
            }
            Source::Npm(npm)
        }
        Source::Brew(mut brew) => {
            if let Some(package) = &f.package {
                brew.package.clone_from(package);
            }
            if let Some(cask) = f.cask {
                brew.cask = cask;
            }
            Source::Brew(brew)
        }
        Source::Script(mut script) => {
            if let Some(command) = &f.update_command {
                script.update_command.clone_from(command);
            }
            script.version_command = optional(f.version_command.as_ref(), script.version_command);
            script.latest_command = optional(f.latest_command.as_ref(), script.latest_command);
            Source::Script(script)
        }
    }
}
