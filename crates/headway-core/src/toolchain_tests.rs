use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn rejects_unpinned_duplicate_and_unsafe_requests() {
    for value in [
        "zig@latest",
        "zig@0.16",
        "zig@0.16.0;rm",
        "zig@0.16.0/other",
        "@0.16.0",
        "--help@1.2.3",
    ] {
        assert!(validate(&[value.into()]).is_err(), "{value}");
    }
    assert!(validate(&["zig@0.16.0".into(), "zig@0.17.0".into()]).is_err());
    assert!(validate(&["zig@0.16.0".into(), "node@24.14.0".into()]).is_ok());
}

#[tokio::test]
async fn selected_zig_overrides_inherited_zig_for_the_child_only() {
    let temp = tempfile::tempdir().unwrap();
    let mise = temp.path().join("mise");
    let zig = temp.path().join("zig");
    std::fs::write(
        &mise,
        "#!/bin/sh\nshift 2\nwhile [ \"$1\" != -- ]; do shift; done\nshift\nexec \"$@\"\n",
    )
    .unwrap();
    std::fs::write(&zig, "#!/bin/sh\nexit 0\n").unwrap();
    for path in [&mise, &zig] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = temp.path().join("chosen");
    let env = ShellEnv::current()
        .with_var("PATH", format!("{}:/usr/bin:/bin", temp.path().display()))
        .with_var("ZIG", "/wrong/zig")
        .with_var("OUTPUT", output.display().to_string())
        .with_var("GITHUB_TOKEN", "secret");
    let cmd = build_command(
        &env,
        "test -z \"${GITHUB_TOKEN+x}\" && printf '%s' \"$ZIG\" > \"$OUTPUT\"",
        &["zig@0.16.0".into()],
    );
    crate::process::run(
        &cmd,
        &tokio_util::sync::CancellationToken::new(),
        &mut |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        zig.display().to_string()
    );
}
