//! Reported Homebrew paths are metadata, not install destinations chosen by Headway.
use super::BrewSource;
use std::path::{Component, Path};

pub(crate) fn absolute_install_path(raw: &str) -> Option<String> {
    let path = Path::new(raw);
    if raw.is_empty()
        || raw.contains(['\0', '\n', '\r'])
        || !path.is_absolute()
        || path.components().any(|part| part == Component::ParentDir)
    {
        return None;
    }
    Some(raw.to_owned())
}

pub(super) fn validate(brew: &BrewSource) -> std::result::Result<(), String> {
    if !super::is_brew_name(&brew.package) {
        return Err(format!(
            "`{}` is not a Homebrew name (use `name` or `user/tap/name`)",
            brew.package
        ));
    }
    if let Some(path) = &brew.install_path
        && absolute_install_path(path).as_deref() != Some(path.as_str())
    {
        return Err(
            "install_path must be an absolute Homebrew path without parent-directory segments"
                .into(),
        );
    }
    Ok(())
}
