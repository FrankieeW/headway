use clap::{Args, ValueEnum};

/// Config fields settable from `add` and `edit`. An empty string clears an optional field.
#[derive(Args, Debug, Default, Clone)]
pub struct AppFields {
    /// Automatic update behavior
    #[arg(long, value_enum)]
    pub update_mode: Option<UpdateModeArg>,
    /// Maximum next-version steps per automatic cycle
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=20))]
    pub max_update_steps: Option<u8>,
    /// Pause automatic updates after failure
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set)]
    pub pause_on_failure: Option<bool>,
    /// Post-install verification command (GitHub only; empty clears)
    #[arg(long, value_name = "COMMAND")]
    pub verify_command: Option<String>,
    /// npm dist-tag to follow (default: latest)
    #[arg(long)]
    pub dist_tag: Option<String>,
    /// Homebrew cask instead of formula
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub cask: Option<bool>,
    /// npm or Homebrew package name
    #[arg(long)]
    pub package: Option<String>,
    /// Official updater command, run with /bin/sh -c (script source)
    #[arg(long)]
    pub update_command: Option<String>,
    /// Command printing the installed version (empty clears it)
    #[arg(long)]
    pub version_command: Option<String>,
    /// Command or URL returning the latest version (empty clears it)
    #[arg(long)]
    pub latest_command: Option<String>,

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
pub enum UpdateModeArg {
    Notify,
    Next,
    Latest,
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
