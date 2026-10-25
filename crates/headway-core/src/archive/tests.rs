use super::*;

fn write_exec(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "#!/bin/sh\necho hi\n").unwrap();
    set_executable(path).unwrap();
}

#[test]
fn detects_archive_kinds() {
    assert_eq!(detect("rg-aarch64-apple-darwin.tar.gz"), ArchiveKind::Tar);
    assert_eq!(detect("x.TGZ"), ArchiveKind::Tar);
    assert_eq!(detect("x.tar.zst"), ArchiveKind::Tar);
    assert_eq!(detect("App.app.zip"), ArchiveKind::Zip);
    assert_eq!(detect("App.dmg"), ArchiveKind::Dmg);
    assert_eq!(detect("jq-macos-arm64"), ArchiveKind::Raw);
}

#[test]
fn finds_named_or_single_binary() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_exec(&root.join("ripgrep-14/rg"));
    fs::write(root.join("ripgrep-14/README.md"), "docs").unwrap();
    assert_eq!(find_binary(root, "rg").unwrap(), root.join("ripgrep-14/rg"));
    // Repo name differs from the binary name: the only executable wins.
    assert_eq!(
        find_binary(root, "ripgrep").unwrap(),
        root.join("ripgrep-14/rg")
    );

    write_exec(&root.join("ripgrep-14/rga"));
    let err = find_binary(root, "ripgrep").unwrap_err();
    assert!(
        matches!(&err, Error::NoAsset(msg) if msg.contains("rg") && msg.contains("set `binary`"))
    );
}

#[test]
fn ignores_files_inside_bundles_and_libraries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_exec(&root.join("Tool.app/Contents/MacOS/Tool"));
    write_exec(&root.join("lib/libtool.dylib"));
    write_exec(&root.join("bin/tool"));
    assert_eq!(find_binary(root, "missing").unwrap(), root.join("bin/tool"));
}

#[test]
fn finds_app_bundles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_exec(&root.join("Zed.app/Contents/MacOS/zed"));
    assert_eq!(find_app_bundle(root, None).unwrap(), root.join("Zed.app"));
    assert_eq!(
        find_app_bundle(root, Some("zed")).unwrap(),
        root.join("Zed.app")
    );
    write_exec(&root.join("nested/Other.app/Contents/MacOS/other"));
    assert!(find_app_bundle(root, None).is_err());
    assert_eq!(
        find_app_bundle(root, Some("Other.app")).unwrap(),
        root.join("nested/Other.app")
    );
    assert!(find_app_bundle(root, Some("Nope")).is_err());
}

#[tokio::test]
async fn unpacks_tarballs_and_raw_binaries() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    write_exec(&src.join("tool-1.0/tool"));
    let tarball = dir.path().join("tool.tar.gz");
    let status = std::process::Command::new("tar")
        .args([
            "-czf",
            &tarball.display().to_string(),
            "-C",
            &src.display().to_string(),
            "tool-1.0",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let env = ShellEnv::current();
    let cancel = CancellationToken::new();
    let out = dir.path().join("out");
    let unpacked = unpack(
        &tarball,
        ArchiveKind::Tar,
        &out,
        &env,
        &cancel,
        &mut |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(
        find_binary(&unpacked.root, "tool").unwrap(),
        out.join("tool-1.0/tool")
    );

    let raw_out = dir.path().join("raw");
    let raw = unpack(
        &src.join("tool-1.0/tool"),
        ArchiveKind::Raw,
        &raw_out,
        &env,
        &cancel,
        &mut |_, _| {},
    )
    .await
    .unwrap();
    assert!(is_executable(&raw.root.join("tool")));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn unpacks_zips_with_ditto() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("Demo.app");
    write_exec(&app.join("Contents/MacOS/Demo"));
    let zip = dir.path().join("Demo.app.zip");
    let status = std::process::Command::new("/usr/bin/ditto")
        .args([
            "-c",
            "-k",
            "--keepParent",
            &app.display().to_string(),
            &zip.display().to_string(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let out = dir.path().join("out");
    let unpacked = unpack(
        &zip,
        ArchiveKind::Zip,
        &out,
        &ShellEnv::current(),
        &CancellationToken::new(),
        &mut |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(
        find_app_bundle(&unpacked.root, None).unwrap(),
        out.join("Demo.app")
    );
}
