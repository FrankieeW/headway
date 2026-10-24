use super::*;

#[tokio::test]
async fn remote_auth_and_missing_branch_errors_preserve_existing_error_types() {
    // Given either an inaccessible page or a missing configured branch tip.
    for (endpoint, status) in [
        ("/repos/acme/project/commits", 401),
        ("/repos/acme/project/commits/main", 404),
    ] {
        let server = MockServer::start().await;
        let (_dir, hw) = headway(
            &server,
            Track::Commit {
                branch: Some("main".into()),
            },
        );
        Mock::given(path(endpoint))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        // When browsing anchored or unresolved history respectively.
        let anchor = (status == 401).then_some(ANCHOR);
        let result = hw.list_commits("selected-app", 1, 2, anchor).await;
        // Then pagination does not mask ordinary repository/auth failures as empty history.
        match status {
            401 => assert!(matches!(result, Err(Error::Auth(_)))),
            404 => assert!(matches!(result, Err(Error::RemoteNotFound(_)))),
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn malformed_remote_tip_never_becomes_a_pagination_anchor() {
    // Given a malformed branch-tip response and no list fixture.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(
        &server,
        Track::Commit {
            branch: Some("main".into()),
        },
    );
    Mock::given(path("/repos/acme/project/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit("short", "Malformed")))
        .expect(1)
        .mount(&server)
        .await;
    // When resolving an unanchored first page.
    let result = hw.list_commits("selected-app", 1, 2, None).await;
    // Then invalid remote data is explicit and no page is queried with a moving/short ref.
    assert!(matches!(result, Err(Error::Parse { .. })));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn next_page_overflow_is_a_controlled_error() {
    // Given a next relation on the maximum representable page number.
    let server = MockServer::start().await;
    let (_dir, hw) = headway(
        &server,
        Track::Commit {
            branch: Some("main".into()),
        },
    );
    page(
        &server,
        u32::MAX,
        json!([]),
        Some("<https://untrusted.invalid/next>; rel=\"next\""),
    )
    .await;
    // When computing the next local page number.
    let result = hw
        .list_commits("selected-app", u32::MAX, 2, Some(ANCHOR))
        .await;
    // Then the API returns an error rather than wrapping or panicking.
    assert!(matches!(result, Err(Error::InvalidInput(_))));
}
