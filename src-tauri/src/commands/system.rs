//! Opening links and folders, and what Headway is.

use tauri::State;

use crate::state::{AppState, CmdResult, invalid};

/// Accepts an absolute `http(s)` URL. Everything else — `file:`, `smb:`,
/// `javascript:`, `data:`, relative paths, whitespace or control characters — is
/// refused before it reaches the system opener.
///
/// Deliberately not restricted to GitHub: npm packages, Homebrew casks/formulae
/// and script sources have legitimate project pages and release notes elsewhere.
pub fn validate_http_url(input: &str) -> CmdResult<String> {
    let url = input.trim();
    if url.is_empty() {
        return Err(invalid("refusing to open an empty link"));
    }
    if url
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(invalid(format!(
            "refusing to open `{url}`: links cannot contain whitespace or control characters"
        )));
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(invalid(format!(
            "refusing to open `{url}`: only http(s) links are allowed"
        )));
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(invalid(format!(
            "refusing to open `{url}`: only http(s) links are allowed"
        )));
    }
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    // IPv6 literals keep their brackets; a bare `host:port` is fine too.
    let host = match authority.find(']') {
        Some(end) => &authority[..=end],
        None => authority.split(':').next().unwrap_or_default(),
    };
    if host.trim_matches(['[', ']']).is_empty() {
        return Err(invalid(format!(
            "refusing to open `{url}`: the link has no host"
        )));
    }
    Ok(url.to_owned())
}

/// Opens an http(s) link in the default browser; anything else is refused.
#[tauri::command]
pub async fn open_url(state: State<'_, AppState>, url: String) -> CmdResult<()> {
    let url = validate_http_url(&url)?;
    state
        .host()?
        .open_url(&url)
        .map_err(|error| invalid(format!("open the link: {error}")))
}

#[tauri::command]
pub async fn reveal_path(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let path = headway_core::paths::expand_tilde(&path);
    if !path.exists() {
        return Err(invalid(format!("{} does not exist", path.display())));
    }
    state
        .host()?
        .reveal(&path)
        .map_err(|error| invalid(format!("reveal in Finder: {error}")))
}

#[tauri::command]
pub async fn app_version() -> String {
    headway_core::Headway::build_info().to_string()
}

#[tauri::command]
pub async fn get_build_info() -> headway_core::BuildInfo {
    headway_core::Headway::build_info()
}
