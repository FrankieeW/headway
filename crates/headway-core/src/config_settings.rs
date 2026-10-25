//! Global configuration settings and string-value parsing.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Global settings shared by the CLI and the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Minutes between background checks in the desktop app; `0` turns them off.
    pub check_interval_minutes: u32,
    /// Notify when new commits or releases show up.
    pub notify: bool,
    /// Where CLI binaries are installed unless an app sets `dest`.
    pub bin_dir: String,
    /// Where `.app` bundles are installed unless an app sets `dest`.
    pub apps_dir: String,
    /// Use `gh auth token` when no token is configured.
    pub gh_fallback: bool,
    /// Desktop language: follow the system, English, or Chinese.
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            check_interval_minutes: 60,
            notify: true,
            bin_dir: "~/.local/bin".to_owned(),
            apps_dir: "/Applications".to_owned(),
            gh_fallback: true,
            language: "system".to_owned(),
        }
    }
}

impl Settings {
    pub const KEYS: [&'static str; 6] = [
        "check_interval_minutes",
        "notify",
        "bin_dir",
        "apps_dir",
        "gh_fallback",
        "language",
    ];

    /// Validates settings loaded from disk as well as API input.
    pub fn validate(&self) -> Result<()> {
        for (key, value) in [("bin_dir", &self.bin_dir), ("apps_dir", &self.apps_dir)] {
            non_empty_path(value).map_err(|err| Error::Config(format!("settings.{key}: {err}")))?;
        }
        // Zero disables checking; the longest useful interval is one year.
        if self.check_interval_minutes > 525_600 {
            return Err(Error::Config(
                "settings.check_interval_minutes must be between 0 and 525600".into(),
            ));
        }
        if !["system", "en", "zh"].contains(&self.language.as_str()) {
            return Err(Error::Config(
                "settings.language must be system, en or zh".into(),
            ));
        }
        Ok(())
    }

    /// Returns a copy with `key` set from its string form (as typed on the command line).
    pub fn with_key(&self, key: &str, value: &str) -> Result<Self> {
        let value = value.trim();
        let next = match key {
            "check_interval_minutes" => Self {
                check_interval_minutes: value.parse().map_err(|_| {
                    Error::InvalidInput(format!("`{value}` is not a number of minutes"))
                })?,
                ..self.clone()
            },
            "notify" => Self {
                notify: parse_bool(value)?,
                ..self.clone()
            },
            "gh_fallback" => Self {
                gh_fallback: parse_bool(value)?,
                ..self.clone()
            },
            "bin_dir" => Self {
                bin_dir: non_empty_path(value)?,
                ..self.clone()
            },
            "language" => Self {
                language: value.to_owned(),
                ..self.clone()
            },
            "apps_dir" => Self {
                apps_dir: non_empty_path(value)?,
                ..self.clone()
            },
            _ => {
                return Err(Error::InvalidInput(format!(
                    "unknown setting `{key}`; expected one of {}",
                    Self::KEYS.join(", ")
                )));
            }
        };
        next.validate()
            .map_err(|err| Error::InvalidInput(err.to_string()))?;
        Ok(next)
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(Error::InvalidInput(format!("`{value}` is not true/false"))),
    }
}

fn non_empty_path(value: &str) -> Result<String> {
    if value.starts_with('/') || value == "~" || value.starts_with("~/") {
        Ok(value.to_owned())
    } else {
        Err(Error::InvalidInput(format!(
            "`{value}` must be an absolute path or start with ~"
        )))
    }
}
