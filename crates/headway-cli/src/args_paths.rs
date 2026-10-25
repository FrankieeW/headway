//! Additive nested command identity; `command` keeps its established group name.
use crate::args::{Command, SettingsCommand, TokenCommand};
use crate::management_args::{CliCommand, SelfCommand};
use crate::revision_args::RepositoryCommand;

impl Command {
    pub const fn subcommand(&self) -> Option<&'static str> {
        Some(match self {
            Self::Token(TokenCommand::Status) => "token status",
            Self::Token(TokenCommand::Set { .. }) => "token set",
            Self::Token(TokenCommand::Clear) => "token clear",
            Self::Settings(SettingsCommand::Get { .. }) => "settings get",
            Self::Settings(SettingsCommand::Set { .. }) => "settings set",
            Self::Cli(CliCommand::Status { .. }) => "cli status",
            Self::Cli(CliCommand::Fix { .. }) => "cli fix",
            Self::SelfUpdate(SelfCommand::Status) => "self status",
            Self::SelfUpdate(SelfCommand::Restart) => "self restart",
            Self::SelfUpdate(SelfCommand::Subscribe) => "self subscribe",
            Self::Repo(RepositoryCommand::List { .. }) => "repo list",
            Self::Repo(RepositoryCommand::Refresh { .. }) => "repo refresh",
            Self::Repo(RepositoryCommand::Add { .. }) => "repo add",
            Self::Repo(RepositoryCommand::Remove { .. }) => "repo remove",
            Self::Repo(RepositoryCommand::Use { .. }) => "repo use",
            _ => return None,
        })
    }
}
