use crate::output::{Printer, Reply};
use crate::revision_args::{RepositoryCommand, RevisionArgs};
use headway_core::{
    CancellationToken, Error, Headway, Install, RepositoryOption, Result, RevisionRequest, Source,
    VariantRequest,
};
use std::fmt::Write as _;

pub async fn commits(
    hw: &Headway,
    id: &str,
    page: u32,
    per_page: u32,
    anchor: Option<&str>,
) -> Result<Reply> {
    let found = hw.list_commits(id, page, per_page, anchor).await?;
    let mut human = String::new();
    for commit in &found.commits {
        let _ = writeln!(
            human,
            "{}  {}",
            headway_core::update::short(&commit.id),
            commit.title
        );
    }
    let next = found
        .next_page
        .map(|next| {
            vec![format!(
                "headway commits {id} --page {next} --per-page {per_page} --anchor {}",
                found.anchor
            )]
        })
        .unwrap_or_default();
    Ok(Reply::new(&found, human).next(next))
}

pub fn source_storage(hw: &Headway, id: &str) -> Result<Reply> {
    let status = hw.source_storage_status(id)?;
    let human = format!(
        "{}  keep={}  exists={}",
        status.path, status.keep, status.exists
    );
    let next = if status.keep {
        vec![format!("headway edit {id} --keep-source false")]
    } else {
        Vec::new()
    };
    Ok(Reply::new(&status, human).next(next))
}

pub async fn options(hw: &Headway, id: &str, reference: &str) -> Result<Reply> {
    let options = hw.revision_options(id, reference).await?;
    let human = format!(
        "{}  release {}  download={}",
        options.commit,
        options.release_tag.as_deref().unwrap_or("none"),
        options.can_download
    );
    Ok(Reply::new(&options, human).next(vec![format!(
        "headway install-commit {id} {} --dry-run",
        if options.pull_request.is_some() {
            &options.reference
        } else {
            &options.commit
        }
    )]))
}

fn request(hw: &Headway, args: &RevisionArgs) -> Result<RevisionRequest> {
    let app = hw.get(&args.id)?.app;
    let github = app.require_github("commit installation")?;
    let mut request = RevisionRequest {
        reference: args.reference.clone(),
        release_tag: args.release_tag.clone(),
        method: args.method.into(),
        source_build: None,
        release_asset: None,
    };
    let source_flags = args.build.is_some() || args.artifact.is_some() || args.workdir.is_some();
    let asset_flags = args.pattern.is_some() || args.binary.is_some();
    let verify_source = args.verify_command.is_some()
        && !asset_flags
        && (source_flags
            || matches!(request.method, headway_core::RevisionMethod::Source)
            || (matches!(request.method, headway_core::RevisionMethod::Configured)
                && matches!(github.install, Install::Source(_))));
    if source_flags || verify_source {
        if matches!(request.method, headway_core::RevisionMethod::Asset) {
            return Err(Error::InvalidInput(
                "source build flags cannot be combined with --method asset".into(),
            ));
        }
        request.method = headway_core::RevisionMethod::Source;
        let mut source = match &github.install {
            Install::Source(s) => s.clone(),
            Install::Asset(_) => github.options.source_build.clone().unwrap_or_default(),
        };
        if let Some(build) = &args.build {
            source.build.clone_from(build);
        }
        if let Some(artifact) = &args.artifact {
            source.artifact = (!artifact.is_empty()).then(|| artifact.clone());
        }
        if let Some(workdir) = &args.workdir {
            source.workdir = (!workdir.is_empty()).then(|| workdir.clone());
        }
        if let Some(command) = &args.verify_command {
            source.verify_command = (!command.is_empty()).then(|| command.clone());
        }
        request.source_build = Some(source);
    }
    if asset_flags || (args.verify_command.is_some() && !verify_source) {
        if matches!(request.method, headway_core::RevisionMethod::Source) {
            return Err(Error::InvalidInput(
                "release asset flags cannot be combined with --method source".into(),
            ));
        }
        request.method = headway_core::RevisionMethod::Asset;
        let mut asset = match &github.install {
            Install::Asset(a) => a.clone(),
            Install::Source(_) => github.options.release_asset.clone().unwrap_or_default(),
        };
        if let Some(pattern) = &args.pattern {
            asset.pattern = (!pattern.is_empty()).then(|| pattern.clone());
        }
        if let Some(binary) = &args.binary {
            asset.binary = (!binary.is_empty()).then(|| binary.clone());
        }
        if let Some(command) = &args.verify_command {
            asset.verify_command = (!command.is_empty()).then(|| command.clone());
        }
        request.release_asset = Some(asset);
    }
    Ok(request)
}

pub async fn install(hw: &Headway, args: &RevisionArgs, printer: &Printer) -> Result<Reply> {
    let request = request(hw, args)?;
    let variant = args.variant_id.as_ref().map(|id| VariantRequest {
        id: id.clone(),
        name: args.name.clone(),
        revision: request.clone(),
    });
    if args.dry_run {
        let plan = if let Some(variant) = &variant {
            hw.plan_variant(&args.id, variant).await?
        } else {
            hw.plan_revision(&args.id, &request).await?
        };
        return Ok(Reply::new(&plan, plan.steps.join("\n")).next(vec![super::without_dry_run()]));
    }
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let watcher = tokio::spawn(async move {
        super::ops::interrupted().await;
        signal.cancel();
    });
    let sink = crate::progress::TerminalSink::new(printer.log_level(), printer.stderr_color);
    let result = if let Some(variant) = &variant {
        hw.install_variant(&args.id, variant, &sink, &cancel).await
    } else {
        hw.install_revision(&args.id, &request, &sink, &cancel)
            .await
    };
    watcher.abort();
    let outcome = result?;
    Ok(
        Reply::new(&outcome, crate::human::outcome(printer.palette, &outcome))
            .next(vec![format!("headway show {}", outcome.app_id)]),
    )
}

pub async fn launch(hw: &Headway, id: &str) -> Result<Reply> {
    let outcome = hw.launch_variant(id).await?;
    Ok(Reply::new(
        &outcome,
        format!("Launched {} (pid {})", outcome.path, outcome.pid),
    ))
}

pub async fn repositories(hw: &Headway, command: RepositoryCommand) -> Result<Reply> {
    match command {
        RepositoryCommand::Refresh { id } => {
            let view = hw.refresh_repositories(&id).await?;
            Ok(
                Reply::new(&view, format!("Discovered repository choices for {id}"))
                    .next(vec![format!("headway repo list {id}")]),
            )
        }
        RepositoryCommand::List { id } => {
            let entries = hw.repositories(&id)?;
            let human = entries
                .iter()
                .map(|e| format!("{}  {}", e.repo, e.label))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Reply::new(&entries, human))
        }
        RepositoryCommand::Use { id, repo } => {
            let view = hw.select_repository(&id, &repo)?;
            Ok(Reply::new(&view,format!("{id} now tracks {repo}; installed bytes retained, revision requires confirmation")).next(vec![format!("headway check {id}")]))
        }
        RepositoryCommand::Add {
            id,
            repo,
            label,
            branch,
            git_url,
        } => {
            let mut entries = hw.repositories(&id)?;
            entries.push(RepositoryOption {
                settings: None,
                label: label.unwrap_or_else(|| repo.clone()),
                repo,
                branch,
                git_url,
            });
            save_repositories(hw, &id, entries)
        }
        RepositoryCommand::Remove { id, repo } => {
            let current = hw.get(&id)?.app.repo_id()?.full_name();
            let requested = headway_core::repo::RepoId::parse(&repo)?.full_name();
            if current.eq_ignore_ascii_case(&requested) {
                return Err(Error::InvalidInput(
                    "select another repository before removing the active one".into(),
                ));
            }
            let entries = hw
                .repositories(&id)?
                .into_iter()
                .filter(|entry| {
                    !headway_core::repo::RepoId::parse(&entry.repo)
                        .is_ok_and(|repo| repo.full_name().eq_ignore_ascii_case(&requested))
                })
                .collect();
            save_repositories(hw, &id, entries)
        }
    }
}

pub fn installation(hw: &Headway, id: &str) -> Result<Reply> {
    let detection = hw.detect_installation(id)?;
    Ok(Reply::new(
        &detection,
        format!(
            "{:?}  {}  baseline_matches={}",
            detection.presence,
            detection.path.as_deref().unwrap_or("unknown path"),
            detection.baseline_matches
        ),
    )
    .next(vec![
        format!("headway show {id}"),
        format!("headway check {id}"),
    ]))
}

pub async fn pull_requests(hw: &Headway, id: &str, page: u32, per_page: u32) -> Result<Reply> {
    let result = hw.list_pull_requests(id, page, per_page).await?;
    let human = result
        .pull_requests
        .iter()
        .map(|pull| format!("#{}  {}  {}", pull.number, pull.head.sha, pull.title))
        .collect::<Vec<_>>()
        .join("\n");
    let mut next: Vec<_> = result
        .pull_requests
        .first()
        .map(|pull| {
            format!(
                "headway install-pr {id} {}@{} --dry-run",
                pull.number, pull.head.sha
            )
        })
        .into_iter()
        .collect();
    if result.has_more
        && let Some(page) = page.checked_add(1)
    {
        next.push(format!(
            "headway prs {id} --page {page} --per-page {per_page}"
        ));
    }
    Ok(Reply::new(&result, human).next(next))
}

fn save_repositories(hw: &Headway, id: &str, entries: Vec<RepositoryOption>) -> Result<Reply> {
    let mut app = hw.get(id)?.app;
    let Source::Github(source) = &mut app.source else {
        return Err(Error::InvalidInput(
            "repository choices require GitHub".into(),
        ));
    };
    source.options.repositories = entries;
    let view = hw.replace(app)?;
    Ok(Reply::new(&view, format!("Saved repositories for {id}")))
}
