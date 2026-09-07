//! Multipart request validation and rejection tests.
use super::*;

#[tokio::test]
async fn zero_file_parts_returns_wrong_file_count() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "wrong_file_count");
}

#[tokio::test]
async fn one_file_part_returns_wrong_file_count() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[file_part("first", "first.txt", "one")]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "wrong_file_count");
}

#[tokio::test]
async fn unknown_field_name_is_rejected() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", "one"),
        file_part("second", "second.txt", "two"),
        file_part("third", "third.txt", "three"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "invalid_field_name");
}

#[tokio::test]
async fn duplicate_named_field_is_rejected() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "one.txt", "one"),
        file_part("first", "two.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "invalid_field_name");
}

#[tokio::test]
async fn non_file_field_is_rejected() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        field_part("note", "not a file"),
        file_part("first", "first.txt", "one"),
        file_part("second", "second.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "non_file_field");
}

#[tokio::test]
async fn oversized_file_returns_413_file_too_large() {
    let app = router(test_state(), "frontend/dist");
    let oversized = vec![b'a'; backend::contract::MAX_FILE_BYTES as usize + 1];
    let body = build_multipart_body(&[
        file_part("first", "first.txt", oversized),
        file_part("second", "second.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_code(response).await, "file_too_large");
}

#[tokio::test]
async fn oversized_request_returns_413_payload_too_large() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", "one"),
        file_part("second", "second.txt", "two"),
    ]);
    let mut req = request(Some(BEARER_TOKEN), false, body);
    req.headers_mut().insert(
        header::CONTENT_LENGTH,
        (backend::contract::MAX_REQUEST_BYTES + 1)
            .to_string()
            .parse()
            .unwrap(),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_code(response).await, "payload_too_large");
}

#[tokio::test]
async fn invalid_utf8_file_is_rejected() {
    let app = router(test_state(), "frontend/dist");
    let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
    let body = build_multipart_body(&[
        file_part("first", "first.txt", invalid_utf8),
        file_part("second", "second.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "invalid_text_encoding");
}

#[tokio::test]
async fn oversized_metadata_is_rejected() {
    let app = router(test_state(), "frontend/dist");
    let long_name: String = "n".repeat(backend::contract::MAX_PART_HEADER_BYTES + 1);
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"a\"; filename=\"{long_name}\"\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"one\r\n");
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());

    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "metadata_too_large");
}

#[tokio::test]
async fn fifth_concurrent_request_returns_service_busy() {
    let state = test_state();
    // Hold every available combine slot before issuing the request under test.
    let mut held_permits = Vec::new();
    for _ in 0..backend::contract::MAX_CONCURRENT_COMBINES {
        held_permits.push(state.combine_slots().clone().try_acquire_owned().unwrap());
    }

    let app = router(state, "frontend/dist");
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error_code(response).await, "service_busy");
    drop(held_permits);
}
