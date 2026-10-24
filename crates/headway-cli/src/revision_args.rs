use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum RevisionMethodArg {
    #[default]
    Configured,
    Source,
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
    pub id: String,
    pub reference: String,
    /// Exact release tag associated with the selected SHA (verified before download)
    #[arg(long)]
    pub release_tag: Option<String>,
    #[arg(long,value_enum,default_value_t=RevisionMethodArg::Configured)]
    pub method: RevisionMethodArg,
    #[arg(long)]
    pub build: Option<String>,
    #[arg(long)]
    pub artifact: Option<String>,
    #[arg(long)]
    pub workdir: Option<String>,
    #[arg(long)]
    pub verify_command: Option<String>,
    #[arg(long)]
    pub pattern: Option<String>,
    #[arg(long)]
    pub binary: Option<String>,
    /// Install alongside the original with this new id (application data remains shared)
    #[arg(long = "as")]
    pub variant_id: Option<String>,
    #[arg(long, requires = "variant_id")]
    pub name: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Subcommand)]
pub enum RepositoryCommand {
    List {
        id: String,
    },
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
    Remove {
        id: String,
        repo: String,
    },
    Use {
        id: String,
        repo: String,
    },
}
