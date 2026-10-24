//! Identity of the running binary, stamped at compile time by the build helper.

use std::fmt;

use serde::Serialize;

/// Build metadata shared by the CLI and desktop app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    pub version: String,
    pub commit: Option<String>,
    pub dirty: bool,
    pub target: Option<String>,
}

impl BuildInfo {
    /// Reads compile-time stamps; never consults the runtime environment or git.
    #[must_use]
    pub fn current() -> Self {
        Self::from_stamps(
            option_env!("HEADWAY_BUILD_SHA"),
            option_env!("HEADWAY_BUILD_DIRTY"),
            option_env!("HEADWAY_BUILD_TARGET"),
        )
    }

    fn from_stamps(commit: Option<&str>, dirty: Option<&str>, target: Option<&str>) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            commit: commit
                .filter(|sha| sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .map(str::to_owned),
            dirty: dirty == Some("true"),
            target: target
                .filter(|target| !target.is_empty())
                .map(str::to_owned),
        }
    }
}

impl fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.version)?;
        if let Some(commit) = &self.commit {
            let short: String = commit.chars().take(7).collect();
            let suffix = if self.dirty { "-dirty" } else { "" };
            write!(f, " ({short}{suffix})")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::BuildInfo;

    const SHA: &str = "abc1234000000000000000000000000000000000";

    #[test]
    fn display_when_clean_or_dirty() {
        for (dirty, suffix) in [("false", ""), ("true", "-dirty")] {
            let info = BuildInfo::from_stamps(Some(SHA), Some(dirty), Some("aarch64-apple-darwin"));
            let display = info.to_string();
            assert_eq!(
                display,
                format!("{} (abc1234{suffix})", env!("CARGO_PKG_VERSION"))
            );
        }
    }

    #[test]
    fn fallback_when_stamps_are_missing_or_invalid() {
        for commit in [
            None,
            Some(""),
            Some("abc1234"),
            Some("🦀"),
            Some("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
        ] {
            let info = BuildInfo::from_stamps(commit, Some("invalid"), Some(""));
            assert_eq!(info.to_string(), env!("CARGO_PKG_VERSION"));
            assert_eq!(info.commit, None);
            assert_eq!(info.target, None);
            assert!(!info.dirty);
        }
    }

    #[test]
    fn serialized_metadata_preserves_full_commit_and_nulls() {
        for (commit, dirty, target) in [
            (Some(SHA), Some("true"), Some("aarch64-apple-darwin")),
            (None, None, None),
        ] {
            let info = BuildInfo::from_stamps(commit, dirty, target);
            let serialized = serde_json::to_value(info).unwrap();
            assert_eq!(
                serialized,
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "commit": commit,
                    "dirty": dirty == Some("true"),
                    "target": target,
                })
            );
        }
    }
}
