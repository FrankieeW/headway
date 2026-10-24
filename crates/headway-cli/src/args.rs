//! Command-line grammar.

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

use crate::output::Format;

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
    version,
    about = "Keep apps and CLI tools built from GitHub current on your Mac: every commit, or every release.",
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
    /// List tracked apps with their cached status
    List,
    /// Show one app: config, installed ref, pending updates, recent history
    Show {
        /// App id
        id: String,
    },
    /// Ask GitHub for new commits or releases (all apps when no id is given)
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
    /// Start tracking a GitHub repository (settings are inferred; flags override)
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
    /// Analyze a repository and print the config `add` would use
    Inspect {
        /// owner/name or a GitHub URL
        repo: String,
    },
    /// List build presets for source installs
    Presets,
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
    /// Install only the next commit/release after the installed one
    #[arg(long, conflicts_with = "to")]
    pub next: bool,
    /// Install this commit SHA, branch or tag (also older ones, to roll back)
    #[arg(long, value_name = "REF", conflicts_with = "all")]
    pub to: Option<String>,
    /// Print the plan without building or installing anything
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// owner/name or a GitHub URL
    pub repo: String,
    /// App id (default: derived from the repository name)
    #[arg(long)]
    pub id: Option<String>,
    /// Do not query GitHub; build the config from flags only
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

/// Config fields settable from `add` and `edit`. An empty string clears an optional field.
#[derive(Args, Debug, Default, Clone)]
pub struct AppFields {
    /// Display name
    #[arg(long)]
    pub name: Option<String>,
    /// What the repository produces
    #[arg(long, value_enum)]
    pub kind: Option<KindArg>,
    /// Follow every commit, GitHub releases, or version tags
    #[arg(long, value_enum)]
    pub track: Option<TrackArg>,
    /// Branch to follow (commit mode; empty = the default branch)
    #[arg(long)]
    pub branch: Option<String>,
    /// Include pre-releases (release/tag mode)
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub prerelease: Option<bool>,
    /// Only consider tags matching this regex (release/tag mode)
    #[arg(long, value_name = "REGEX")]
    pub tag_filter: Option<String>,
    /// Build from source, or download a release asset
    #[arg(long, value_enum)]
    pub method: Option<MethodArg>,
    /// Fill the build command and artifact from a preset (see `headway presets`)
    #[arg(long)]
    pub preset: Option<String>,
    /// Build command, run with `/bin/sh -c` in the checkout
    #[arg(long, value_name = "COMMAND")]
    pub build: Option<String>,
    /// Subdirectory of the repository to build in
    #[arg(long, value_name = "DIR")]
    pub workdir: Option<String>,
    /// Build output to install, relative to the workdir (`*` allowed; empty = the build installs itself)
    #[arg(long, value_name = "PATH")]
    pub artifact: Option<String>,
    /// Install destination (default: `bin_dir` or `apps_dir` + file name)
    #[arg(long, value_name = "PATH")]
    pub dest: Option<String>,
    /// Release asset glob, e.g. '*aarch64-apple-darwin.tar.gz' (asset method; empty = automatic)
    #[arg(long, value_name = "GLOB")]
    pub pattern: Option<String>,
    /// Binary or .app name inside the release asset (asset method)
    #[arg(long, value_name = "NAME")]
    pub binary: Option<String>,
    /// Let the desktop app install updates as soon as it finds them
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub auto_update: Option<bool>,
    /// Clone URL override, e.g. git@github.com:owner/repo.git (empty clears)
    #[arg(long, value_name = "URL")]
    pub git_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KindArg {
    Cli,
    App,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TrackArg {
    Commit,
    Release,
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MethodArg {
    Source,
    Asset,
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
