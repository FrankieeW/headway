use super::{MatchConfidence, MatchEvidence, MatchEvidenceKind as Kind, evidence, metadata};
use serde_json::json;

#[test]
fn formula_metadata_keeps_taps_urls_and_linked_or_single_keg() {
    let data = json!({"formulae":[
        {"name":"ripgrep", "full_name":"ripgrep", "homepage":"https://github.com/BurntSushi/ripgrep",
         "urls":{"stable":{"url":"https://github.com/BurntSushi/ripgrep/archive/v15.tar.gz"},"head":{"url":"https://github.com/burntsushi/RIPGREP.git"}},
         "installed":[{"version":"14"},{"version":"15"}],"linked_keg":"15"},
        {"name":"tool","tap":"acme/tap","installed":[{"version":"1"}]},
        {"name":"ambiguous","installed":[{"version":"1"},{"version":"2"}],"linked_keg":"3"}
    ]});
    let packages = metadata::parse(&data.to_string()).unwrap();
    assert_eq!(packages[0].installed.as_deref(), Some("15"));
    let repos = evidence::repositories(packages[0].repositories());
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].repo, "BurntSushi/ripgrep");
    assert_eq!(
        repos[0].evidence.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [Kind::SourceUrl, Kind::HeadUrl, Kind::Homepage]
    );
    assert_eq!(repos[0].confidence, MatchConfidence::High);
    assert_eq!(packages[1].package, "acme/tap/tool");
    assert_eq!(packages[1].installed.as_deref(), Some("1"));
    assert_eq!(packages[2].installed, None);
}

#[test]
fn cask_metadata_uses_full_token_display_name_download_and_installed() {
    let packages = metadata::parse(&json!({"casks":[
        {"token":"zed@preview","full_token":"zed@preview","tap":"homebrew/cask", "name":["Zed Preview"],"installed":"1.2","homepage":"https://zed.dev/","url":"https://github.com/zed-industries/zed/releases/a.dmg"},
        {"token":"tool","tap":"acme/tap","name":[],"installed":""},
        {"token":"tool","full_token":"other/tap/tool","tap":"acme/tap"}
    ]}).to_string()).unwrap();
    assert!(packages[0].cask);
    assert_eq!(packages[0].name, "Zed Preview");
    assert_eq!(packages[0].installed.as_deref(), Some("1.2"));
    assert_eq!(packages[0].repositories()[0].1.kind, Kind::DownloadUrl);
    assert_eq!(packages[1].package, "acme/tap/tool");
    assert_eq!(packages[1].name, "tool");
    assert_eq!(packages[1].installed, None);
    assert_eq!(packages[2].package, "other/tap/tool");
}

#[test]
fn evidence_is_deduplicated_ordered_and_confidence_is_derived() {
    let evidence = vec![
        MatchEvidence {
            kind: Kind::Search,
            value: "tool".into(),
        },
        MatchEvidence {
            kind: Kind::Name,
            value: "tool".into(),
        },
        MatchEvidence {
            kind: Kind::Search,
            value: "tool".into(),
        },
    ];
    let low = evidence::ordered(evidence);
    assert_eq!(low.len(), 2);
    assert_eq!(low[0].kind, Kind::Name);
    assert_eq!(evidence::confidence(&low), MatchConfidence::Low);
    let mut medium = low;
    medium.push(MatchEvidence {
        kind: Kind::HomepageMatch,
        value: "https://tool.dev".into(),
    });
    assert_eq!(evidence::confidence(&medium), MatchConfidence::Medium);
    medium.push(MatchEvidence {
        kind: Kind::HeadUrl,
        value: "https://github.com/acme/tool.git".into(),
    });
    assert_eq!(evidence::confidence(&medium), MatchConfidence::High);
    assert!(MatchConfidence::High < MatchConfidence::Medium);
    assert!(MatchConfidence::Medium < MatchConfidence::Low);
}

#[test]
fn search_ranking_prefers_exact_versions_then_affixes_and_filters_noise() {
    let output = "==> Formulae\nother\nmytool\ntool-extra\nacme/tap/tool\nold-tool\ntool@2\ntool\n--bad\n/regex/\ntool\n";
    assert_eq!(
        super::search::rank_names(output, &["tool".into()]),
        ["acme/tap/tool", "tool", "tool@2", "old-tool", "tool-extra"]
    );
    assert_eq!(
        super::search::terms("Obsidian-releases"),
        ["obsidian-releases", "obsidian"]
    );
    assert_eq!(
        super::search::terms("Zulip-desktop"),
        ["zulip-desktop", "zulip"]
    );
    for invalid in ["-flag", "/regex/", "a b", "a&b", &"a".repeat(65)] {
        assert!(!super::search::valid_term(invalid));
    }
}
