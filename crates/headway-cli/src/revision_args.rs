use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum RevisionMethodArg {
    /// Use the app's saved install method and recipe
    #[default]
    Configured,
    /// Build from the selected GitHub revision
    Source,
    /// Download an asset from a verified matching release
    Asset,
}
impl From<RevisionMethodArg> for headway_core::RevisionMethod {
    fn from(value: RevisionMethodArg) -> Self {
        match value {
            RevisionMethodArg::Configured => Self::Configured,
            RevisionMethodArg::Source => Self::Source,
            RevisionMethodArg::Asset => Self::Asset,
        }
    }
}

#[derive(Debug, Args)]
pub struct RevisionArgs {
    /// Tracked GitHub app id
    pub id: String,
    /// Commit/tag, or PR number[@sha] for install-pr (pr:number@sha is also accepted)
    pub reference: String,
    /// Exact release tag associated with the selected SHA (verified before download)
    #[arg(long)]
    pub release_tag: Option<String>,
    /// Use the configured recipe, a source build, or a verified release asset
    #[arg(long, value_enum, default_value_t = RevisionMethodArg::Configured)]
    pub method: RevisionMethodArg,
    /// Source build command, run with /bin/sh -c in the checkout
    #[arg(long)]
    pub build: Option<String>,
    /// Source build output relative to workdir (empty clears it)
    #[arg(long)]
    pub artifact: Option<String>,
    /// Source build subdirectory (empty clears it)
    #[arg(long)]
    pub workdir: Option<String>,
    /// Post-install verification command (empty clears it)
    #[arg(long)]
    pub verify_command: Option<String>,
    /// Release asset glob (empty selects automatically)
    #[arg(long)]
    pub pattern: Option<String>,
    /// Binary or .app name inside the release asset
    #[arg(long)]
    pub binary: Option<String>,
    /// Install alongside the original with this new id (application data remains shared)
    #[arg(long = "as")]
    pub variant_id: Option<String>,
    /// Display name of the independently installed variant (requires --as)
    #[arg(long, requires = "variant_id")]
    pub name: Option<String>,
    /// Validate and show the plan without building or installing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Subcommand)]
pub enum RepositoryCommand {
    /// Discover and save parent/root fork choices without activating them
    Refresh { id: String },
    /// List saved official and fork repository choices
    List { id: String },
    /// Add a repository choice without activating it
    Add {
        id: String,
        repo: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        git_url: Option<String>,
    },
    /// Remove an inactive repository choice
    Remove { id: String, repo: String },
    /// Activate a saved repository choice without replacing installed bytes
    Use { id: String, repo: String },
}
