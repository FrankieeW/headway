use super::*;
use crate::testing::FakeTools;

fn formula_json(installed: &str, outdated: bool) -> String {
    format!(
        r#"{{"formulae":[{{"name":"ripgrep","desc":"Search tool","homepage":"https://x","versions":{{"stable":"15.2.0"}},"revision":1,"installed":[{installed}],"outdated":{outdated},"pinned":false}}],"casks":[]}}"#
    )
}

fn fake_brew(info: &str) -> FakeTools {
    FakeTools::new().tool(
        "brew",
        &format!("case \"$1\" in info) echo '{info}';; *) echo \"brew $*\";; esac"),
    )
}

fn ripgrep() -> BrewSource {
    BrewSource {
        install_path: None,
        package: "ripgrep".into(),
        cask: false,
    }
}

#[tokio::test]
async fn outdated_formulae_offer_the_current_version() {
    let tools = fake_brew(&formula_json(r#"{"version":"15.1.0"}"#, true));
    let checked = check(&CancellationToken::new(), &tools.env(), &ripgrep())
        .await
        .unwrap();
    assert_eq!(checked.result.status, UpdateStatus::UpdateAvailable);
    assert_eq!(checked.result.latest.unwrap().id, "15.2.0_1");
    assert_eq!(checked.result.pending_total, 1);
    assert_eq!(
        checked.installed,
        InstalledSync::Detected(Some("15.1.0".into()))
    );
}

#[tokio::test]
async fn current_and_missing_formulae() {
    let current = fake_brew(&formula_json(r#"{"version":"15.2.0_1"}"#, false));
    assert_eq!(
        check(&CancellationToken::new(), &current.env(), &ripgrep())
            .await
            .unwrap()
            .result
            .status,
        UpdateStatus::UpToDate
    );
    let missing = fake_brew(&formula_json("", false));
    let checked = check(&CancellationToken::new(), &missing.env(), &ripgrep())
        .await
        .unwrap();
    assert_eq!(checked.result.status, UpdateStatus::NotInstalled);
    assert_eq!(checked.installed, InstalledSync::Detected(None));
}

#[tokio::test]
async fn casks_and_lookups() {
    let info = r#"{"formulae":[],"casks":[{"token":"zed","name":["Zed"],"desc":"Editor","version":"0.200.1","installed":"0.199.0","outdated":true}]}"#;
    let tools = fake_brew(info);
    let source = BrewSource {
        install_path: None,
        package: "zed".into(),
        cask: true,
    };
    let checked = check(&CancellationToken::new(), &tools.env(), &source)
        .await
        .unwrap();
    assert_eq!(checked.result.latest.unwrap().id, "0.200.1");
    let found = lookup(&CancellationToken::new(), &tools.env(), "zed", true)
        .await
        .unwrap();
    assert_eq!(found.suggestion.name, "Zed");
    assert_eq!(found.suggestion.kind, AppKind::App);
    assert!(found.notes[1].contains("upgrade is available"));
}

#[tokio::test]
async fn unknown_names_are_not_found() {
    let tools = FakeTools::new().tool(
        "brew",
        "echo 'Error: No available formula with the name \"nope\".' >&2; exit 1",
    );
    let source = BrewSource {
        install_path: None,
        package: "nope".into(),
        cask: false,
    };
    assert!(matches!(
        check(&CancellationToken::new(), &tools.env(), &source).await,
        Err(Error::RemoteNotFound(_))
    ));
}

#[tokio::test]
async fn real_homebrew_missing_package_errors_are_not_found() {
    for (cask, stderr) in [
        (
            false,
            "Error: No available formula with the name \"zz-typo\".",
        ),
        (
            true,
            "Error: Cask 'zz-typo' is unavailable: No Cask with this name exists.",
        ),
    ] {
        let tools = FakeTools::new().tool(
            "brew",
            &format!(
                "printf '%s\\n' {} >&2; exit 1",
                process::shell_quote(stderr)
            ),
        );
        let source = BrewSource {
            package: "zz-typo".into(),
            cask,
            install_path: None,
        };
        let error = status(&CancellationToken::new(), &tools.env(), &source)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "remote_not_found", "{stderr}");
        assert_eq!(error.exit_code(), 3);
    }
}

#[test]
fn update_commands() {
    let env = ShellEnv::from_vars([]);
    let cask = BrewSource {
        install_path: None,
        package: "zed".into(),
        cask: true,
    };
    assert_eq!(
        update_cmd(&env, &cask, true).display(),
        "brew upgrade --cask zed"
    );
    assert_eq!(
        update_cmd(&env, &ripgrep(), false).display(),
        "brew install --formula ripgrep"
    );
}

#[tokio::test]
async fn linked_keg_wins_over_array_order_and_unlinked_versions_sort_numerically() {
    let base = formula_json(r#"{"version":"15.2.0"},{"version":"9.0.0"}"#, false);
    let linked = base.replace(
        "\"outdated\":false",
        "\"linked_keg\":\"9.0.0\",\"outdated\":false",
    );
    for (info, expected) in [(linked, "9.0.0"), (base, "15.2.0")] {
        let tools = fake_brew(&info);
        let checked = check(&CancellationToken::new(), &tools.env(), &ripgrep())
            .await
            .unwrap();
        assert_eq!(
            checked.installed,
            InstalledSync::Detected(Some(expected.into()))
        );
    }
}
