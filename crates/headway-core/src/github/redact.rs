//! Sanitizing untrusted diagnostics: bounded text, and URL redaction.
//!
//! GitHub error bodies and transport errors can contain the request URL (including
//! any query string) and arbitrary repository-provided text. Neither may reach a
//! log, terminal or UI verbatim: control characters are escaped, text is bounded,
//! and user info plus queries are stripped from URLs.

use crate::error::Error;

/// Bounded, control-character-free rendering of untrusted text for diagnostics.
pub(super) fn display_text(value: &str) -> String {
    const LIMIT: usize = 120;
    let mut out = String::with_capacity(value.len().min(LIMIT));
    for (index, character) in value.chars().enumerate() {
        if index >= LIMIT {
            out.push('…');
            return out;
        }
        out.push(if character.is_control() {
            '?'
        } else {
            character
        });
    }
    out
}

/// Removes query strings, fragments and user info from every URL in `text`.
pub(super) fn scrub_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(start) = url_start(rest) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let end = candidate
            .find(|character: char| character.is_whitespace() || ")]}\"'".contains(character))
            .unwrap_or(candidate.len());
        out.push_str(&redact_url(&candidate[..end]));
        rest = &candidate[end..];
    }
}

/// Maps a transport failure, with any queried URL redacted from its message.
///
/// The error is consumed through [`reqwest::Error::without_url`] (its own `Display`
/// would otherwise print the full URL, query string included); the redacted endpoint
/// is appended afterwards so diagnostics keep the host and path but never the query.
pub(super) fn network(err: reqwest::Error) -> Error {
    let endpoint = err.url().map(|url| redact_url(url.as_str()));
    let message = err.without_url().to_string();
    let message = match endpoint {
        Some(endpoint) => format!("{message} ({endpoint})"),
        None => message,
    };
    Error::Network(scrub_urls(&message))
}

fn url_start(text: &str) -> Option<usize> {
    ["https://", "http://"]
        .iter()
        .filter_map(|scheme| text.find(scheme))
        .min()
}

fn redact_url(token: &str) -> String {
    let Some(scheme_end) = token.find("://").map(|index| index + 3) else {
        return token.to_owned();
    };
    let scheme = &token[..scheme_end];
    let after = &token[scheme_end..];
    let authority_end = after.find('/').unwrap_or(after.len());
    let authority = &after[..authority_end];
    let (prefix, remainder) = match authority.rfind('@') {
        Some(at) => (format!("{scheme}<redacted>@"), &after[at + 1..]),
        None => (scheme.to_owned(), after),
    };
    let cut = remainder.find(['?', '#']);
    let mut out = prefix;
    match cut {
        Some(cut) => {
            out.push_str(&remainder[..cut]);
            out.push_str("?<redacted>");
        }
        None => out.push_str(remainder),
    }
    out
}
