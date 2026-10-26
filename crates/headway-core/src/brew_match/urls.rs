//! Strict metadata URL identities; never infer repositories from Pages or documentation hosts.
use crate::repo::RepoId;
use reqwest::Url;

const RESERVED: &[&str] = &[
    "about",
    "account",
    "advisories",
    "apps",
    "codespaces",
    "collections",
    "customer-stories",
    "dashboard",
    "enterprise",
    "events",
    "explore",
    "features",
    "issues",
    "join",
    "login",
    "logout",
    "marketplace",
    "new",
    "notifications",
    "orgs",
    "organizations",
    "pricing",
    "pulls",
    "readme",
    "resources",
    "search",
    "security",
    "settings",
    "signup",
    "site",
    "solutions",
    "sponsors",
    "stars",
    "team",
    "topics",
    "trending",
    "user-attachments",
    "users",
    "watching",
];

fn parse_url(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if let Some((host, path)) = raw.strip_prefix("git@").and_then(|s| s.split_once(':'))
        && host.eq_ignore_ascii_case("github.com")
    {
        return Url::parse(&format!("ssh://git@github.com/{path}")).ok();
    }
    Url::parse(raw).ok()
}

pub(super) fn github_repo_in_url(raw: &str) -> Option<RepoId> {
    let url = parse_url(raw)?;
    match url.scheme() {
        "https" | "http" | "git" => {}
        "ssh" if url.username() == "git" && url.password().is_none() => {}
        _ => return None,
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let mut parts = url.path().strip_prefix('/')?.split('/');
    if host == "api.github.com" && parts.next()? != "repos" {
        return None;
    }
    let mut owner = parts.next()?;
    let mut name = parts.next()?;
    match host.as_str() {
        "github.com" | "www.github.com" => {
            if owner.eq_ignore_ascii_case("downloads") {
                owner = name;
                name = parts.next()?;
                if parts.next()?.is_empty() {
                    return None;
                }
            }
            if RESERVED
                .iter()
                .any(|route| owner.eq_ignore_ascii_case(route))
            {
                return None;
            }
        }
        "codeload.github.com" | "raw.githubusercontent.com" | "api.github.com" => {}
        _ => return None,
    }
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    RepoId::parse(&format!("{owner}/{name}")).ok()
}

pub(super) fn normalize_homepage(raw: &str) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let port = url
        .port()
        .map_or_else(String::new, |port| format!(":{port}"));
    Some(format!("{host}{port}{}", url.path().trim_end_matches('/')))
}

pub(super) fn sanitize_url(raw: &str) -> Option<String> {
    let mut url = parse_url(raw)?;
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    Some(url.as_str().chars().take(300).collect())
}
