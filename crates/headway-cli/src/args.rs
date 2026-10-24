//! Command-line grammar.

pub use crate::args_fields::{AppFields, KindArg, MethodArg, TrackArg, UpdateModeArg};
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::output::Format;

fn build_version() -> &'static str {
    static VERSION: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| headway_core::Headway::build_info().to_string());
    VERSION.as_str()
}

const AFTER_HELP: &str = "\
Examples:
  headway add BurntSushi/ripgrep        track every commit, build with cargo (inferred)
  headway add zed-industries/zed --track release --kind app
  headway check                         look for new commits / releases
  headway update rg --next              install only the next commit after the installed one
  headway update --all                  install everything that is behind
  headway --ai list                     XML for AI agents (--json for JSON)
  headway commands --json               every command and flag, as data

Exit codes: 0 ok, 1 error, 2 usage, 3 not found, 4 GitHub/network, 5 build/install failed,
6 busy, 7 cancelled.";

#[derive(Parser, Debug)]
#[allow(clippy::struct_excessive_bools)] // independent global flags
#[command(
    name = "headway",
    version = build_version(),
    about = "Keep Mac apps and CLI tools current via GitHub, npm, Homebrew or official scripts.",
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// Result format on stdout
    #[arg(long, value_enum, global = true, env = "HEADWAY_OUTPUT", default_value_t = Format::Human)]
    pub output: Format,
    /// Same as `--output json`
    #[arg(long, global = true)]
    pub json: bool,
    /// Same as `--output xml`, the format meant for AI agents
    #[arg(long, global = true)]
    pub ai: bool,
    /// No progress or build output on stderr
    #[arg(long, short, global = true, conflicts_with = "verbose")]
    pub quiet: bool,
    /// Stream full build output on stderr, also in --json/--ai mode
    #[arg(long, short, global = true)]
    pub verbose: bool,
    /// Disable colors (`NO_COLOR` is honoured too)
    #[arg(long, global = true)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub const fn format(&self) -> Format {
        if self.ai {
            Format::Xml
        } else if self.json {
            Format::Json
        } else {
            self.output
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show the running binary version and compile-time build metadata
    Version,
    /// List tracked apps with their cached status
    List,
    /// Show one app: config, installed ref, pending updates, recent history
    Show {
        /// App id
        id: String,
    },
    /// Check configured sources for updates (all apps when no id is given)
    Check {
        /// App ids
        ids: Vec<String>,
    },
    /// Build or download a version and install it
    Update(UpdateArgs),
    /// Record a version as installed without building it (set a baseline)
    Mark {
        /// App id
        id: String,
        /// `latest`, a commit SHA or a tag
        #[arg(default_value = "latest")]
        reference: String,
    },
    /// Start tracking a GitHub, npm, Homebrew or script source (settings are inferred; flags override)
    Add(AddArgs),
    /// Change how an app is tracked, built or installed
    Edit(EditArgs),
    /// Stop tracking an app
    Remove {
        /// App id
        id: String,
        /// Also delete the cached source checkout and logs
        #[arg(long)]
        purge: bool,
        /// Show what would be removed
        #[arg(long)]
        dry_run: bool,
    },
    /// Inspect a GitHub repository or look up an npm/Homebrew package
    Inspect {
        /// owner/name, npm:package, brew:formula, cask:cask, script:name, or a source URL
        repo: String,
    },
    /// List build presets for source installs
    Presets,
    /// Discover installed brew/npm packages without updating them
    Discover {
        #[arg(long = "source", value_delimiter = ',')]
        sources: Vec<String>,
    },
    /// Track selected discovery keys (or recipe:<id>)
    Import {
        #[arg(required = true)]
        keys: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// List editable application recipes
    Recipes,
    /// Resume automatic updates after a failure pause
    ResumeAutoUpdate { id: String },
    /// Run one bounded cycle using each app's saved automatic update policy
    AutoUpdate { ids: Vec<String> },
    /// List retained prior artifacts
    Backups { id: String },
    /// Restore a retained artifact without rebuilding
    Restore { id: String, backup_id: String },
    /// Inspect or repair the CLI link after moving Headway.app
    #[command(subcommand)]
    Cli(crate::management_args::CliCommand),
    /// Inspect, restart, or subscribe to Headway itself
    #[command(name = "self", subcommand)]
    SelfUpdate(crate::management_args::SelfCommand),
    /// Read version, CI and release metadata for a GitHub commit
    CommitInfo {
        id: String,
        sha: String,
        #[arg(long)]
        refresh: bool,
    },
    /// Install history of an app, newest first
    History {
        /// App id
        id: String,
        /// Maximum entries
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print the most recent update log of an app
    Log {
        /// App id
        id: String,
        /// Only the last N lines
        #[arg(long)]
        tail: Option<usize>,
    },
    /// Manage the GitHub token (stored in the macOS Keychain)
    #[command(subcommand)]
    Token(TokenCommand),
    /// Read or change global settings
    #[command(subcommand)]
    Settings(SettingsCommand),
    /// Check git, the token, PATH and build tools
    Doctor,
    /// Print config, state, cache and log locations
    Paths,
    /// Describe every command and flag as data (for agents and scripts)
    Commands,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

impl Command {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::List => "list",
            Self::Show { .. } => "show",
            Self::Check { .. } => "check",
            Self::Update(_) => "update",
            Self::Mark { .. } => "mark",
            Self::Add(_) => "add",
            Self::Edit(_) => "edit",
            Self::Remove { .. } => "remove",
            Self::Inspect { .. } => "inspect",
            Self::Presets => "presets",
            Self::Discover { .. } => "discover",
            Self::Import { .. } => "import",
            Self::Recipes => "recipes",
            Self::ResumeAutoUpdate { .. } => "resume-auto-update",
            Self::AutoUpdate { .. } => "auto-update",
            Self::Backups { .. } => "backups",
            Self::Restore { .. } => "restore",
            Self::Cli(_) => "cli",
            Self::SelfUpdate(_) => "self",
            Self::CommitInfo { .. } => "commit-info",
            Self::History { .. } => "history",
            Self::Log { .. } => "log",
            Self::Token(_) => "token",
            Self::Settings(_) => "settings",
            Self::Doctor => "doctor",
            Self::Paths => "paths",
            Self::Commands => "commands",
            Self::Completions { .. } => "completions",
        }
    }
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// App ids
    #[arg(conflicts_with = "all")]
    pub ids: Vec<String>,
    /// Check every app and update the ones that are behind
    #[arg(long)]
    pub all: bool,
    /// Install the next GitHub commit/release or npm version
    #[arg(long, conflicts_with = "to")]
    pub next: bool,
    /// Install a GitHub ref or npm version (also older ones, to roll back)
    #[arg(long, value_name = "REF", conflicts_with = "all")]
    pub to: Option<String>,
    /// Print the plan without building or installing anything
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// owner/name, npm:package, brew:formula, cask:cask, script:name, or a source URL
    pub repo: String,
    /// App id (default: derived from the source name)
    #[arg(long)]
    pub id: Option<String>,
    /// Skip remote inspection; build the config from source defaults and flags
    #[arg(long)]
    pub no_inspect: bool,
    /// Print the resulting config without saving it
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub fields: AppFields,
}

#[derive(Args, Debug)]
pub struct EditArgs {
    /// App id
    pub id: String,
    /// Print the resulting config without saving it
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub fields: AppFields,
}

#[derive(Subcommand, Debug)]
pub enum TokenCommand {
    /// Which token is used, whom it belongs to, and the rate limit
    Status,
    /// Store a token in the keychain (read from stdin or a hidden prompt)
    Set {
        /// Read the token from stdin
        #[arg(long, conflicts_with = "from_gh")]
        stdin: bool,
        /// Copy the token of the logged-in GitHub CLI (`gh auth token`)
        #[arg(long)]
        from_gh: bool,
        /// Store without asking GitHub whether the token works
        #[arg(long)]
        no_verify: bool,
    },
    /// Delete the token from the keychain
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum SettingsCommand {
    /// Print all settings, or one
    Get {
        /// Setting name
        key: Option<String>,
    },
    /// Change a setting
    Set {
        /// `check_interval_minutes`, `notify`, `bin_dir`, `apps_dir` or `gh_fallback`
        key: String,
        value: String,
    },
}
