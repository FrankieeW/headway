use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    Status {
        #[arg(long)]
        app: Option<String>,
    },
    Fix {
        #[arg(long)]
        app: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum SelfCommand {
    Status,
    Restart,
    Subscribe,
}
