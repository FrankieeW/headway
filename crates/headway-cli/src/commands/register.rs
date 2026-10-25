//! Source defaults and registration.
use super::{apps::toml_of, without_dry_run};
use crate::args::{AddArgs, AppFields, KindArg, MethodArg, TrackArg};
use crate::human::Palette;
use crate::output::Reply;
use crate::{fields, human};
use headway_core::config::{Install, SourceInstall, Track, TrackedApp};
use headway_core::inspect::RepoInspection;
use headway_core::repo::slugify;
use headway_core::{
    AppKind, BrewSource, Error, GithubSource, Headway, NpmSource, Result, ScriptSource, Source,
    SourceSpec,
};
use serde_json::json;
use std::fmt::Write as _;
pub async fn add(hw: &Headway, args: &AddArgs, p: Palette) -> Result<Reply> {
    let spec = SourceSpec::parse(&args.repo)?;
    let defaults = bare_app(&spec);
    crate::source_fields::validate(&defaults.source, &args.fields)?;
    let (base, notes, fields) = if args.no_inspect {
        (defaults, Vec::new(), args.fields.clone())
    } else {
        match spec {
            SourceSpec::Github(_) => {
                let found = hw.inspect(&args.repo).await?;
                let fields = prefer_release_asset(&args.fields, &found);
                (found.suggestion, found.notes, fields)
            }
            SourceSpec::Npm(_) | SourceSpec::Brew { .. } => {
                let configured = fields::apply(defaults, &args.fields)?;
                let input = match &configured.source {
                    Source::Npm(npm) => format!("npm:{}", npm.package),
                    Source::Brew(brew) => format!(
                        "{}:{}",
                        if brew.cask { "cask" } else { "brew" },
                        brew.package
                    ),
                    Source::Github(_) | Source::Script(_) => {
                        return Err(Error::InvalidInput(
                            "package lookup requires npm or Homebrew".into(),
                        ));
                    }
                };
                let found = hw.lookup(&input).await?;
                (found.suggestion, found.notes, args.fields.clone())
            }
            SourceSpec::Script(_) => (defaults, Vec::new(), args.fields.clone()),
        }
    };
    let base = match &args.id {
        Some(id) => TrackedApp {
            id: id.clone(),
            ..base
        },
        None => base,
    };
    let app = fields::apply(base.clone(), &fields)?;
    let notes = match (base.github(), app.github()) {
        (Some(before), Some(after))
            if !notes.is_empty()
                && (before.track.mode() != after.track.mode()
                    || before.install.method() != after.install.method()) =>
        {
            vec![format!(
                "Flags override the suggested setup (follows {}, {}).",
                human::source_mode(&base),
                human::source_install(&base)
            )]
        }
        _ => notes,
    };

    app.validate().map_err(|err| with_add_hint(err, &app))?;

    let mut human = String::new();
    if args.dry_run {
        let _ = writeln!(human, "{}", p.dim("dry run, nothing saved:"));
        human.push_str(&toml_of(&app));
        append_notes(&mut human, &notes);
        let data = json!({"app": app, "notes": notes, "saved": false});
        return Ok(Reply::new(&data, human).next(vec![without_dry_run()]));
    }
    let view = hw.add(app)?;
    let id = view.app.id.clone();
    let _ = writeln!(
        human,
        "{} tracking {} as `{id}`: follows {}, {}",
        p.green("✓"),
        view.app.source.label(),
        human::source_mode(&view.app),
        human::source_install(&view.app)
    );
    append_notes(&mut human, &notes);
    let data = json!({"app": view.app, "notes": notes, "saved": true});
    let mut next = vec![
        format!("headway check {id}"),
        format!("headway update {id} --dry-run"),
    ];
    if view.app.github().is_some() {
        next.push(format!("headway mark {id} latest"));
    }
    Ok(Reply::new(&data, human).next(next))
}

/// `--track release` without `--method` means "download it" when the release has a macOS asset.
fn prefer_release_asset(fields: &AppFields, found: &RepoInspection) -> AppFields {
    let asset_kind = found
        .latest_release
        .as_ref()
        .and_then(|release| release.suggested_kind);
    match (fields.track, fields.method, asset_kind) {
        (Some(TrackArg::Release), None, Some(kind)) => AppFields {
            method: Some(MethodArg::Asset),
            kind: fields.kind.or(Some(match kind {
                AppKind::Cli => KindArg::Cli,
                AppKind::App => KindArg::App,
            })),
            ..fields.clone()
        },
        _ => fields.clone(),
    }
}

/// The starting point for `add --no-inspect`: everything must come from flags.
fn bare_app(spec: &SourceSpec) -> TrackedApp {
    let (name, source, kind) = match spec {
        SourceSpec::Github(repo) => (
            repo.name.clone(),
            Source::Github(GithubSource {
                options: Box::default(),
                repo: repo.full_name(),
                git_url: None,
                track: Track::Commit { branch: None },
                install: Install::Source(SourceInstall::default()),
            }),
            AppKind::Cli,
        ),
        SourceSpec::Npm(package) => (
            package.clone(),
            Source::Npm(NpmSource {
                package: package.clone(),
                dist_tag: "latest".into(),
            }),
            AppKind::Cli,
        ),
        SourceSpec::Brew { package, cask } => (
            package.clone(),
            Source::Brew(BrewSource {
                install_path: None,
                package: package.clone(),
                cask: *cask,
            }),
            if *cask { AppKind::App } else { AppKind::Cli },
        ),
        SourceSpec::Script(name) => (
            name.clone(),
            Source::Script(ScriptSource {
                update_command: String::new(),
                version_command: None,
                latest_command: None,
            }),
            AppKind::Cli,
        ),
    };
    TrackedApp {
        id: slugify(name.rsplit('/').next().unwrap_or(&name)),
        name,
        kind,
        auto_update: false,
        update_policy: headway_core::policy::UpdatePolicy::default(),
        source,
    }
}

fn with_add_hint(err: Error, app: &TrackedApp) -> Error {
    match (&err, app.github().map(|github| &github.install)) {
        (Error::Config(message), Some(Install::Source(source)))
            if source.build.trim().is_empty() =>
        {
            Error::InvalidInput(format!(
                "{message}; pass --build '<command>' or --preset <id> (see `headway presets`), or --track release --method asset"
            ))
        }
        _ => err,
    }
}

fn append_notes(out: &mut String, notes: &[String]) {
    for note in notes {
        let _ = writeln!(out, "  • {note}");
    }
}
