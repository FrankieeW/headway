//! Error type shared by the engine, the CLI and the desktop app.
//!
//! Every variant maps to a stable machine-readable [`Error::code`], a process
//! [`Error::exit_code`] and an optional [`Error::hint`], so agents can branch on
//! failures without parsing prose.

use std::path::Path;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("app `{0}` is not tracked")]
    AppNotFound(String),
    #[error("app `{0}` is already tracked")]
    AppExists(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid config: {0}")]
    Config(String),
    #[error("GitHub rejected the request: {0}")]
    Auth(String),
    #[error("GitHub API rate limit exceeded (resets at {reset})")]
    RateLimited { reset: String },
    #[error("not found: {0}")]
    RemoteNotFound(String),
    #[error("GitHub API error (HTTP {status}): {message}")]
    GitHub { status: u16, message: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("no installable release asset: {0}")]
    NoAsset(String),
    #[error("checksum mismatch for {asset}: expected {expected}, got {actual}")]
    Checksum {
        asset: String,
        expected: String,
        actual: String,
    },
    #[error("`{command}` failed{}", exit_suffix(*.status))]
    CommandFailed {
        command: String,
        status: Option<i32>,
        /// Last lines of output, to show why it failed without opening the log.
        tail: Vec<String>,
    },
    #[error("`{0}` is busy: another update of it is running")]
    Busy(String),
    #[error("cancelled")]
    Cancelled,
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {what}: {message}")]
    Parse { what: String, message: String },
}

/// Serializable form of an [`Error`], returned to the UI and printed by the CLI.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Last output lines of a failed command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

impl From<&Error> for ErrorInfo {
    fn from(err: &Error) -> Self {
        Self {
            code: err.code().to_owned(),
            message: err.to_string(),
            hint: err.hint(),
            details: match err {
                Error::CommandFailed { tail, .. } => tail.clone(),
                _ => Vec::new(),
            },
        }
    }
}

fn exit_suffix(status: Option<i32>) -> String {
    status.map_or_else(
        || " (terminated by a signal)".to_owned(),
        |code| format!(" with exit code {code}"),
    )
}

impl Error {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// I/O error annotated with the action and the path it happened on.
    pub fn io_at(action: &str, path: &Path, source: std::io::Error) -> Self {
        Self::io(format!("{action} {}", path.display()), source)
    }

    pub fn parse(what: impl Into<String>, message: &dyn std::fmt::Display) -> Self {
        Self::Parse {
            what: what.into(),
            message: message.to_string(),
        }
    }

    /// Stable identifier for the failure class, safe to match on.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::AppNotFound(_) => "app_not_found",
            Self::AppExists(_) => "app_exists",
            Self::InvalidInput(_) => "invalid_input",
            Self::Config(_) => "invalid_config",
            Self::Auth(_) => "auth_failed",
            Self::RateLimited { .. } => "rate_limited",
            Self::RemoteNotFound(_) => "remote_not_found",
            Self::GitHub { .. } => "github_error",
            Self::Network(_) => "network_error",
            Self::NoAsset(_) => "no_asset",
            Self::Checksum { .. } => "checksum_mismatch",
            Self::CommandFailed { .. } => "command_failed",
            Self::Busy(_) => "busy",
            Self::Cancelled => "cancelled",
            Self::Keychain(_) => "keychain_error",
            Self::Io { .. } => "io_error",
            Self::Parse { .. } => "parse_error",
        }
    }

    /// Process exit code the CLI uses for this failure.
    ///
    /// `2` usage, `3` not found, `4` GitHub/network, `5` build or install, `6` busy,
    /// `7` cancelled, `1` everything else.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) | Self::AppExists(_) => 2,
            Self::AppNotFound(_) | Self::RemoteNotFound(_) | Self::NoAsset(_) => 3,
            Self::Auth(_) | Self::RateLimited { .. } | Self::GitHub { .. } | Self::Network(_) => 4,
            Self::CommandFailed { .. } | Self::Checksum { .. } => 5,
            Self::Busy(_) => 6,
            Self::Cancelled => 7,
            Self::Config(_) | Self::Keychain(_) | Self::Io { .. } | Self::Parse { .. } => 1,
        }
    }

    /// A suggested next step, phrased as a command where possible.
    pub fn hint(&self) -> Option<String> {
        let hint = match self {
            Self::AppNotFound(_) => "run `headway list` to see the tracked app ids",
            Self::AppExists(_) => {
                "pick another id with `--id`, or change it with `headway edit <id>`"
            }
            Self::Auth(_) => {
                "set a valid token with `headway token set`; fine-grained tokens need read access to Contents and Metadata"
            }
            Self::RateLimited { .. } => {
                "configure a GitHub token with `headway token set` to raise the limit to 5000 requests/hour"
            }
            Self::RemoteNotFound(_) => {
                "check the repository, package, branch, channel or version; private GitHub repositories need a token that can read them"
            }
            Self::NoAsset(_) => {
                "set an asset glob with `headway edit <id> --pattern '<glob>'`, or build from source with `--method source`"
            }
            Self::CommandFailed { .. } => "read the full output with `headway log <id>`",
            Self::Busy(_) => {
                "wait for the running update to finish, or cancel it in the Headway app"
            }
            Self::Network(_) => "check the network connection or proxy settings",
            Self::Checksum { .. } => {
                "the download was corrupted or tampered with; retry, and report it upstream if it persists"
            }
            Self::Config(_) | Self::Parse { .. } => {
                "fix the file shown above (see `headway paths`), or edit the app with `headway edit`"
            }
            Self::Keychain(_) => {
                "unlock the login keychain, or export HEADWAY_GITHUB_TOKEN instead"
            }
            Self::InvalidInput(_) | Self::GitHub { .. } | Self::Cancelled | Self::Io { .. } => {
                return None;
            }
        };
        Some(hint.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_failed_message_mentions_exit_status() {
        let err = Error::CommandFailed {
            command: "cargo build".into(),
            status: Some(101),
            tail: vec![],
        };
        assert_eq!(err.to_string(), "`cargo build` failed with exit code 101");
        assert_eq!(err.code(), "command_failed");
        assert_eq!(err.exit_code(), 5);
    }

    #[test]
    fn signal_termination_is_described() {
        let err = Error::CommandFailed {
            command: "make".into(),
            status: None,
            tail: vec![],
        };
        assert!(err.to_string().ends_with("(terminated by a signal)"));
    }

    #[test]
    fn codes_and_exit_codes_are_distinct_per_class() {
        assert_eq!(Error::AppNotFound("x".into()).exit_code(), 3);
        assert_eq!(Error::Busy("x".into()).exit_code(), 6);
        assert_eq!(Error::Cancelled.exit_code(), 7);
        assert_eq!(Error::InvalidInput("x".into()).exit_code(), 2);
        assert_eq!(
            Error::RateLimited {
                reset: "soon".into()
            }
            .code(),
            "rate_limited"
        );
        assert!(Error::AppNotFound("x".into()).hint().is_some());
        assert!(Error::Cancelled.hint().is_none());
    }
}
