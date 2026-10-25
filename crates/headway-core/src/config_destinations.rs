//! Destinations knowable without downloading or building an artifact.
use std::path::{Component, Path, PathBuf};

use super::{AppKind, Config, Install, Settings, TrackedApp};
use crate::{Error, Result};

/// Unknown bundle/glob output names are checked when the actual payload is selected.
pub(crate) fn destination(app: &TrackedApp, settings: &Settings) -> Option<PathBuf> {
    let github = app.github()?;
    let (configured, name) = match &github.install {
        Install::Source(source) => (
            source.dest.as_deref(),
            source.artifact.as_deref().and_then(|artifact| {
                let name = Path::new(artifact).file_name()?.to_str()?;
                (!name.contains(['*', '?'])).then(|| name.to_owned())
            }),
        ),
        Install::Asset(asset) => (
            asset.dest.as_deref(),
            asset.binary.as_ref().map(|binary| {
                if app.kind == AppKind::App
                    && !Path::new(binary)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
                {
                    format!("{binary}.app")
                } else {
                    binary.clone()
                }
            }),
        ),
    };
    configured
        .map(crate::paths::expand_tilde)
        .or_else(|| name.map(|name| crate::install::resolve_dest(None, app.kind, &name, settings)))
}

/// Lexical comparison avoids probing external volumes while holding the store lock.
pub(crate) fn same_destination(left: &Path, right: &Path) -> bool {
    fn normalized(path: &Path) -> PathBuf {
        let mut output = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    output.pop();
                }
                other => output.push(other.as_os_str()),
            }
        }
        output
    }
    normalized(left) == normalized(right)
}

pub(crate) fn validate_destination(config: &Config, id: &str, dest: &Path) -> Result<()> {
    if let Some(other) = config.apps.iter().filter(|app| app.id != id).find(|app| {
        destination(app, &config.settings).is_some_and(|path| same_destination(&path, dest))
    }) {
        return Err(Error::Config(format!(
            "apps `{id}` and `{}` share destination `{}`; use `headway paths` and give them independent destinations in config.toml",
            other.id,
            dest.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{cli_app, with_github};
    #[test]
    fn collisions_name_both_apps_and_the_manual_repair_path() {
        let first = cli_app("first");
        let second = with_github(cli_app("second"), |mut github| {
            if let Install::Source(source) = &mut github.install {
                source.artifact = Some("target/release/first".into());
            }
            github
        });
        let error = Config {
            settings: Settings::default(),
            apps: vec![first, second],
        }
        .validate()
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("`first`") && message.contains("`second`"));
        assert!(message.contains("config.toml") && message.contains("headway paths"));
    }
    #[test]
    fn bundle_binary_names_match_execution_and_unknown_asset_names_are_not_invented() {
        let app = with_github(cli_app("tool"), |mut github| {
            github.install = Install::Asset(crate::AssetInstall {
                binary: Some("Tool".into()),
                ..Default::default()
            });
            github
        });
        let app = TrackedApp {
            kind: AppKind::App,
            ..app
        };
        assert_eq!(
            destination(&app, &Settings::default()).unwrap(),
            PathBuf::from("/Applications/Tool.app")
        );
        let unknown = with_github(app, |mut github| {
            github.install = Install::Asset(crate::AssetInstall::default());
            github
        });
        assert!(destination(&unknown, &Settings::default()).is_none());
    }
}
