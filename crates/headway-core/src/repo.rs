//! GitHub repository identifiers and app id helpers.

use std::fmt;

use crate::error::{Error, Result};

/// `owner/name` of a GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoId {
    pub owner: String,
    pub name: String,
}

impl RepoId {
    /// Parses `owner/name`, `https://github.com/owner/name[.git][/...]`,
    /// `git@github.com:owner/name.git` or `ssh://git@github.com/owner/name`.
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim().trim_end_matches('/');
        let path = trimmed
            .strip_prefix("git@github.com:")
            .or_else(|| {
                trimmed
                    .find("github.com/")
                    .map(|at| &trimmed[at + "github.com/".len()..])
            })
            .unwrap_or(trimmed);

        let mut parts = path.split('/').filter(|part| !part.is_empty());
        let (Some(owner), Some(name)) = (parts.next(), parts.next()) else {
            return Err(invalid(input));
        };
        let name = name.strip_suffix(".git").unwrap_or(name);
        if !is_valid_owner(owner) || !is_valid_name(name) {
            return Err(invalid(input));
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn https_clone_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.name)
    }

    pub fn html_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }
}

impl fmt::Display for RepoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

fn invalid(input: &str) -> Error {
    Error::InvalidInput(format!(
        "`{input}` is not a GitHub repository; use owner/name or a github.com URL"
    ))
}

fn is_valid_owner(owner: &str) -> bool {
    (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_valid_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Turns a display name into an app id: lowercase ASCII, digits and single dashes.
pub fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug: String = slug.chars().take(64).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "app".to_owned()
    } else {
        slug.to_owned()
    }
}

/// App ids end up in file names (clones, locks, logs), so they are restricted to
/// `[a-z0-9][a-z0-9._-]{0,63}`.
pub fn validate_app_id(id: &str) -> Result<()> {
    let mut chars = id.chars();
    let starts_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest_ok =
        chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if starts_ok && rest_ok && id.len() <= 64 {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "app id `{id}` must match [a-z0-9][a-z0-9._-]{{0,63}}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_repository_spellings() {
        let expected = RepoId {
            owner: "BurntSushi".into(),
            name: "ripgrep".into(),
        };
        for input in [
            "BurntSushi/ripgrep",
            " BurntSushi/ripgrep/ ",
            "https://github.com/BurntSushi/ripgrep",
            "https://github.com/BurntSushi/ripgrep.git",
            "https://github.com/BurntSushi/ripgrep/tree/master/crates",
            "github.com/BurntSushi/ripgrep",
            "git@github.com:BurntSushi/ripgrep.git",
            "ssh://git@github.com/BurntSushi/ripgrep.git",
        ] {
            assert_eq!(RepoId::parse(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn rejects_malformed_repositories() {
        for input in [
            "",
            "ripgrep",
            "/ripgrep",
            "-bad/name",
            "owner/..",
            "own er/repo",
            "o/re po",
        ] {
            assert!(RepoId::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn builds_urls() {
        let repo = RepoId::parse("tauri-apps/tauri").unwrap();
        assert_eq!(repo.full_name(), "tauri-apps/tauri");
        assert_eq!(
            repo.https_clone_url(),
            "https://github.com/tauri-apps/tauri.git"
        );
        assert_eq!(repo.html_url(), "https://github.com/tauri-apps/tauri");
        assert_eq!(repo.to_string(), "tauri-apps/tauri");
    }

    #[test]
    fn slugify_produces_valid_ids() {
        assert_eq!(slugify("ripgrep"), "ripgrep");
        assert_eq!(slugify("My Cool App!"), "my-cool-app");
        assert_eq!(slugify("  --Zed.Editor-- "), "zed-editor");
        assert_eq!(slugify("工具"), "app");
        assert_eq!(slugify(&"a".repeat(80)).len(), 64);
        for input in ["ripgrep", "My Cool App!", "工具", "x---y"] {
            validate_app_id(&slugify(input)).unwrap();
        }
    }

    #[test]
    fn app_ids_cannot_escape_directories() {
        assert!(validate_app_id("rg").is_ok());
        assert!(validate_app_id("zed.dev-nightly_1").is_ok());
        for bad in [
            "",
            "../x",
            "a/b",
            "Upper",
            "-lead",
            ".hidden",
            &"a".repeat(65),
        ] {
            assert!(validate_app_id(bad).is_err(), "{bad}");
        }
    }
}
