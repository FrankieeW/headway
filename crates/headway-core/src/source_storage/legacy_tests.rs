use super::*;

#[tokio::test]
async fn nonempty_custom_git_checkout_requires_headway_ownership() {
    // Given a manually cloned repository at the selected custom child.
    let f = Fixture::new(true, true, BUILD).await;
    let dir = f.base.join("tool");
    let env = f.hw.env().await;
    let cancel = CancellationToken::new();
    crate::git::Checkout {
        dir: &dir,
        env: &env,
        cancel: &cancel,
    }
    .sync(
        f.app.github().unwrap().git_url.as_deref().unwrap(),
        &mut |_, _| {},
    )
    .await
    .unwrap();
    fs::write(dir.join("user-data"), "keep").unwrap();
    // When updating or purging that unmarked user checkout.
    assert!(f.update().await.is_err());
    assert!(f.hw.remove("tool", true).is_err());
    // Then matching Git origin alone cannot claim a custom directory.
    assert_eq!(fs::read(dir.join("user-data")).unwrap(), b"keep");
    assert!(dir.join(".git").is_dir());
}

#[tokio::test]
async fn default_legacy_checkout_with_another_origin_is_refused() {
    // Given an unmarked default-cache repository with the wrong origin.
    let f = Fixture::new(true, false, BUILD).await;
    let other = GitFixture::new();
    let dir = f.hw.paths().source_dir("tool");
    let env = f.hw.env().await;
    let cancel = CancellationToken::new();
    crate::git::Checkout {
        dir: &dir,
        env: &env,
        cancel: &cancel,
    }
    .sync(&other.url(), &mut |_, _| {})
    .await
    .unwrap();
    fs::write(dir.join("user-data"), "keep").unwrap();
    // When migration cannot prove that it is the configured cache clone.
    assert!(f.update().await.is_err());
    assert!(f.hw.remove("tool", true).is_err());
    // Then no claim, reuse, deletion, or origin rewrite occurs.
    assert_eq!(fs::read(dir.join("user-data")).unwrap(), b"keep");
    assert!(
        !dir.parent()
            .unwrap()
            .join(".tool.headway-source.json")
            .exists()
    );
}

async fn github_legacy(f: &Fixture, origin: &str, expected: &str) -> PathBuf {
    let dir = f.hw.paths().source_dir("tool");
    let env = f.hw.env().await;
    let local = f.app.github().unwrap().git_url.as_deref().unwrap();
    crate::git::Checkout {
        dir: &dir,
        env: &env,
        cancel: &CancellationToken::new(),
    }
    .sync(local, &mut |_, _| {})
    .await
    .unwrap();
    // Rewrite fetches to a real local repository, leaving the displayed origin intact.
    for args in [
        vec![
            "remote".into(),
            "set-url".into(),
            "origin".into(),
            origin.into(),
        ],
        vec![
            "config".into(),
            format!("url.{local}.insteadOf"),
            expected.into(),
        ],
    ] {
        let output = crate::process::capture(
            &crate::process::Cmd::new("git", &env)
                .arg("-C")
                .arg(dir.display().to_string())
                .args(args),
        )
        .await
        .unwrap();
        assert!(output.success());
    }
    let app = with_github(f.app.clone(), |mut github| {
        github.git_url = Some(expected.into());
        github
    });
    f.hw.replace(app).unwrap();
    dir
}

#[tokio::test]
async fn github_case_and_transport_aliases_migrate_the_real_default_cache() {
    // Given genuine legacy clones whose displayed GitHub origins vary in case/transport.
    for (origin, expected) in [
        (
            "https://github.com/OWNER/Tool.git",
            "https://github.com/owner/tool.git",
        ),
        (
            "git@github.com:OwNeR/ToOl.git",
            "https://github.com/owner/tool.git",
        ),
        (
            "https://github.com/OWNER/Tool.git",
            "git@github.com:owner/tool.git",
        ),
    ] {
        let f = Fixture::new(true, false, BUILD).await;
        let dir = github_legacy(&f, origin, expected).await;
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target/cache"), "legacy-cache").unwrap();
        // When the public update API fetches/builds through local Git URL rewriting.
        let outcome = f.update().await.unwrap();
        // Then the existing checkout/cache is reused and the exact requested SHA installs.
        assert_eq!(f.checkout(), dir.canonicalize().unwrap());
        assert_eq!(fs::read_to_string(&f.cache).unwrap().trim(), "warm");
        assert_eq!(outcome.to.sha.as_deref(), Some(f.sha.as_str()));
        println!(
            "github_legacy_migration={}",
            serde_json::json!({"origin":origin,"expected":expected,"sha":outcome.to.sha,"cache":"warm"})
        );
    }
}

#[tokio::test]
async fn foreign_github_legacy_origin_is_refused_with_existing_bytes_preserved() {
    // Given a legacy clone claiming a different GitHub repository.
    let f = Fixture::new(true, false, BUILD).await;
    let dir = github_legacy(
        &f,
        "git@github.com:other/tool.git",
        "https://github.com/owner/tool.git",
    )
    .await;
    fs::write(dir.join("user-data"), "precious").unwrap();
    let git_config = fs::read(dir.join(".git/config")).unwrap();
    let destination = f.root.path().join("bin/tool");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, "existing-installed-bytes").unwrap();
    // When public update admission cannot prove the legacy clone's identity.
    let error = f.update().await.unwrap_err();
    // Then neither cache data nor the existing payload is replaced or claimed.
    assert!(matches!(error, Error::InvalidInput(_)));
    assert_eq!(fs::read(dir.join("user-data")).unwrap(), b"precious");
    assert_eq!(fs::read(&destination).unwrap(), b"existing-installed-bytes");
    assert_eq!(fs::read(dir.join(".git/config")).unwrap(), git_config);
    assert!(
        f.hw.store()
            .state()
            .unwrap()
            .app("tool")
            .installed
            .is_none()
    );
    assert!(
        !dir.parent()
            .unwrap()
            .join(".tool.headway-source.json")
            .exists()
    );
    println!(
        "github_legacy_foreign_refused={}",
        serde_json::json!({
            "cache_bytes":fs::read_to_string(dir.join("user-data")).unwrap(),
            "installed_bytes":fs::read_to_string(destination).unwrap(),
            "git_config_preserved":true,"installed":f.hw.store().state().unwrap().app("tool").installed
        })
    );
}
