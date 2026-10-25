use super::*;

#[tokio::test]
async fn all_supported_token_sources_are_removed() {
    let env = ShellEnv::current()
        .with_var("HEADWAY_GITHUB_TOKEN", "one")
        .with_var("GITHUB_TOKEN", "two")
        .with_var("GH_TOKEN", "three");
    let out = capture(&Cmd::new("/bin/sh", &env).args(["-c",
        "printf '%s|%s|%s' \"${HEADWAY_GITHUB_TOKEN:-unset}\" \"${GITHUB_TOKEN:-unset}\" \"${GH_TOKEN:-unset}\""])).await.unwrap();
    assert_eq!(out.stdout, "unset|unset|unset");
}

#[tokio::test]
async fn command_and_capture_deadlines_terminate_children() {
    let cmd = Cmd::new("/bin/sh", &ShellEnv::current())
        .args(["-c", "sleep 30 & wait"])
        .timeout(Duration::from_millis(30));
    let start = std::time::Instant::now();
    let error = run(&cmd, &CancellationToken::new(), &mut |_, _| {})
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::CommandFailed { tail, .. } if tail.iter().any(|line| line.contains("timed out")))
    );
    let error = capture(&cmd).await.unwrap_err();
    assert!(
        matches!(error, Error::CommandFailed { tail, .. } if tail.iter().any(|line| line.contains("timed out")))
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn capture_rejects_oversized_output_without_buffering_it_all() {
    let cmd = Cmd::new("/usr/bin/head", &ShellEnv::current()).args(["-c", "18000000", "/dev/zero"]);
    let error = capture(&cmd).await.unwrap_err();
    assert!(error.to_string().contains("16777216 bytes"));
}
