use super::same_origin;

#[test]
fn proven_github_origins_compare_canonical_case_across_clone_transports() {
    // Given standard endpoints for one GitHub repository.
    let canonical = "https://github.com/Owner/Tool.git";
    // When comparing their repository identities.
    for remote in [
        "https://GITHUB.COM/OWNER/TOOL.git/",
        "http://github.com/owner/tool",
        "git@github.com:owner/tool.git",
        "git@GITHUB.COM:OWNER/Tool",
        "ssh://git@github.com/owner/tool.git",
        "ssh://git@github.com:22/Owner/Tool.git",
        "git://github.com/owner/tool.git",
    ] {
        // Then case and supported transport do not create a foreign-origin failure.
        assert!(same_origin(remote, canonical), "{remote}");
    }
}

#[test]
fn unknown_hosts_paths_and_non_github_aliases_keep_strict_raw_equality() {
    // Given unrelated, deceptive, or unsupported endpoints.
    let github = "https://github.com/owner/tool.git";
    for remote in [
        "https://github.com/other/tool.git",
        "https://github.com/owner/other.git",
        "https://evil.example/github.com/owner/tool.git",
        "https://github.com.evil.example/owner/tool.git",
        "https://github.com@evil.example/owner/tool.git",
        "git@github.com.evil.example:owner/tool.git",
        "https://github.com:8443/owner/tool.git",
        "https://github.com/owner/tool.git?other=repository",
        "https://github.com/owner/tool.git#other",
        "https://github.com/owner/tool/tree/main",
        "https://github.com//owner/tool.git",
        "ssh://other@github.com/owner/tool.git",
        "owner/tool",
    ] {
        // When compared against a known GitHub origin, none can claim its identity.
        assert!(!same_origin(remote, github), "{remote}");
        // Then an otherwise unknown remote still retains the previous exact-match contract.
        assert!(same_origin(remote, remote));
    }
    assert!(!same_origin(
        "git@other.example:Owner/Tool.git",
        "https://other.example/owner/tool.git"
    ));
    assert!(!same_origin(
        "file:///tmp/Owner/Tool",
        "file:///tmp/owner/tool"
    ));
}
