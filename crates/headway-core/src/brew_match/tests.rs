use super::urls::{github_repo_in_url, normalize_homepage, sanitize_url};

#[test]
fn legacy_download_urls_extract_the_actual_repository() {
    for (url, expected) in [
        (
            "https://github.com/downloads/Owner/Repo/file.zip",
            Some("Owner/Repo"),
        ),
        (
            "https://www.github.com/downloads/Owner/Repo/file.zip",
            Some("Owner/Repo"),
        ),
        ("https://github.com/downloads/Owner", None),
        ("https://github.com/downloads/Owner/Repo", None),
        ("https://github.com/downloads//Repo/file.zip", None),
        ("https://github.com/downloads/Owner/re%70o/file.zip", None),
        (
            "https://codeload.github.com/downloads/Repo/tar.gz/main",
            Some("downloads/Repo"),
        ),
    ] {
        assert_eq!(
            github_repo_in_url(url).map(|r| r.full_name()).as_deref(),
            expected,
            "{url}"
        );
    }
}

#[test]
fn non_repository_github_routes_are_rejected() {
    for route in "account advisories codespaces customer-stories dashboard join logout readme resources security signup solutions stars team user-attachments watching".split_whitespace() {
        for spelling in [route.to_owned(), route.to_uppercase()] {
            let url = format!("https://github.com/{spelling}/repo");
            assert!(github_repo_in_url(&url).is_none(), "{url}");
        }
    }
}

#[test]
fn github_url_extraction_real_homebrew_examples() {
    for (url, repo) in [
        (
            "https://github.com/BurntSushi/ripgrep",
            Some("BurntSushi/ripgrep"),
        ),
        (
            "https://github.com/BurntSushi/ripgrep/archive/refs/tags/15.2.0.tar.gz",
            Some("BurntSushi/ripgrep"),
        ),
        (
            "https://github.com/BurntSushi/ripgrep.git",
            Some("BurntSushi/ripgrep"),
        ),
        ("https://cli.github.com/", None),
        (
            "https://github.com/cli/cli/archive/refs/tags/v2.103.2.tar.gz",
            Some("cli/cli"),
        ),
        ("https://dandavison.github.io/delta/", None),
        (
            "https://github.com/dandavison/delta/archive/refs/tags/0.18.tar.gz",
            Some("dandavison/delta"),
        ),
        ("https://zed.dev/", None),
        (
            "https://github.com/zed-industries/zed/releases/download/v1.25.3/Zed-aarch64.dmg",
            Some("zed-industries/zed"),
        ),
        ("https://docs.github.com/en/copilot/...", None),
        (
            "https://github.com/github/copilot-cli/releases/download/...",
            Some("github/copilot-cli"),
        ),
        ("https://clash-verge-rev.github.io/", None),
        (
            "https://github.com/clash-verge-rev/clash-verge-rev/releases/download/v1/a.dmg",
            Some("clash-verge-rev/clash-verge-rev"),
        ),
        (
            "https://github.com/RSSNext/Folo/releases/download/desktop%2Fv1.16.0/...",
            Some("RSSNext/Folo"),
        ),
        ("https://github.com/google/fonts.git", Some("google/fonts")),
    ] {
        assert_eq!(
            github_repo_in_url(url).map(|r| r.full_name()).as_deref(),
            repo,
            "{url}"
        );
    }
}

#[test]
fn github_url_hosts_schemes_routes_and_encoded_names_are_strict() {
    for url in [
        "http://WWW.GITHUB.COM/Owner/Repo/tree/main",
        "git://GitHub.com/Owner/Repo.git",
        "ssh://git@GITHUB.com/Owner/Repo.git",
        "git@github.com:Owner/Repo.git",
        "https://codeload.github.com/Owner/Repo/tar.gz/main",
        "https://raw.githubusercontent.com/Owner/Repo/main/README.md",
        "https://api.github.com/repos/Owner/Repo/releases",
    ] {
        assert_eq!(
            github_repo_in_url(url).unwrap().full_name(),
            "Owner/Repo",
            "{url}"
        );
    }
    for url in [
        "https://gist.github.com/owner/repo",
        "https://objects.githubusercontent.com/owner/repo",
        "https://desktop.githubusercontent.com/owner/repo",
        "https://github.githubassets.com/owner/repo",
        "https://github.com.evil.test/owner/repo",
        "https://evil.test/github.com/owner/repo",
        "https://api.github.com/owner/repo",
        "ftp://github.com/owner/repo",
        "ssh://root@github.com/owner/repo",
        "ssh://git:secret@github.com/owner/repo",
        "https://github.com/owner/re%70o",
        "https://github.com/ow%6eer/repo",
        "owner/repo",
        "https://github.com/owner",
        "https://github.com//owner/repo",
    ] {
        assert!(github_repo_in_url(url).is_none(), "{url}");
    }
    for route in "about apps collections enterprise events explore features issues login marketplace new notifications orgs organizations pricing pulls search settings site sponsors topics trending users".split_whitespace() {
        assert!(github_repo_in_url(&format!("https://github.com/{}/repo", route.to_uppercase())).is_none());
    }
}

#[test]
fn homepage_normalization_uses_exact_host_and_path() {
    for url in ["https://zed.dev/", "http://www.ZED.dev?x=y#z"] {
        assert_eq!(normalize_homepage(url).as_deref(), Some("zed.dev"));
    }
    assert_eq!(
        normalize_homepage("https://www.Example.com/docs///?a=b#x").as_deref(),
        Some("example.com/docs")
    );
    assert_ne!(
        normalize_homepage("https://zed.dev/docs"),
        normalize_homepage("https://zed.dev")
    );
    for url in ["git://zed.dev", "zed.dev", "file:///zed.dev"] {
        assert!(normalize_homepage(url).is_none());
    }
}

#[test]
fn evidence_urls_remove_secrets_and_are_bounded() {
    assert_eq!(
        sanitize_url("https://user:secret@github.com/acme/tool?token=secret#secret").as_deref(),
        Some("https://github.com/acme/tool")
    );
    assert_eq!(
        sanitize_url("git@github.com:acme/tool.git").as_deref(),
        Some("ssh://github.com/acme/tool.git")
    );
    let long = format!("https://example.com/{}?secret=yes", "a".repeat(400));
    let clean = sanitize_url(&long).unwrap();
    assert_eq!(clean.chars().count(), 300);
    assert!(!clean.contains("secret"));
}
