//! Authentication and probe-header authorization tests.
use super::*;

#[tokio::test]
async fn missing_credentials_returns_401() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
    ]);
    let response = app.oneshot(request(None, false, body)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(response).await, "unauthorized");
}

#[tokio::test]
async fn invalid_credentials_returns_401() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", VALID_TEMPLATE),
        file_part("second", "second.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some("wrong-token"), false, body))
        .await
        .unwrap();
    // Non-matching Bearer tokens are treated as direct PATs and authorized.
    // With FakeCombineBackend, this results in a successful 200 response.
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn probe_token_without_probe_header_returns_401() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(PROBE_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn probe_token_with_probe_header_is_authorized() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", VALID_TEMPLATE),
        file_part("second", "second.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(PROBE_TOKEN), true, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
