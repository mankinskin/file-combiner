//! HTTP integration tests for the authenticated `/api/combine` endpoint.
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use backend::{router, AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const BEARER_TOKEN: &str = "test-group-token";
const PROBE_TOKEN: &str = "test-probe-token";
const BOUNDARY: &str = "combine-test-boundary";

struct Part {
    name: &'static str,
    file_name: Option<&'static str>,
    content: Vec<u8>,
}

fn file_part(name: &'static str, file_name: &'static str, content: impl Into<Vec<u8>>) -> Part {
    Part {
        name,
        file_name: Some(file_name),
        content: content.into(),
    }
}

fn field_part(name: &'static str, content: impl Into<Vec<u8>>) -> Part {
    Part {
        name,
        file_name: None,
        content: content.into(),
    }
}

fn build_multipart_body(parts: &[Part]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match part.file_name {
            Some(file_name) => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                        part.name, file_name
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(b"Content-Type: text/plain\r\n\r\n");
            }
            None => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                        part.name
                    )
                    .as_bytes(),
                );
            }
        }
        body.extend_from_slice(&part.content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn request(auth: Option<&str>, probe_header: bool, body: Vec<u8>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/combine")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        );
    if let Some(auth) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {auth}"));
    }
    if probe_header {
        builder = builder.header("x-combine-probe", "1");
    }
    builder.body(Body::from(body)).unwrap()
}

fn test_state() -> AppState {
    AppState::new(BEARER_TOKEN.to_string(), PROBE_TOKEN.to_string())
}

async fn error_code(response: axum::response::Response) -> String {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&body).unwrap();
    let obj = value.as_object().unwrap();
    assert_eq!(
        obj.len(),
        2,
        "error body must contain only code and message"
    );
    obj.get("code").unwrap().as_str().unwrap().to_string()
}

#[tokio::test]
async fn missing_credentials_returns_401() {
    let app = router(test_state());
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
    let app = router(test_state());
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some("wrong-token"), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn probe_token_without_probe_header_returns_401() {
    let app = router(test_state());
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
    let app = router(test_state());
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
    ]);
    let response = app
        .oneshot(request(Some(PROBE_TOKEN), true, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn two_valid_files_combine_byte_exact_with_attachment_headers() {
    let app = router(test_state());
    let body = build_multipart_body(&[
        file_part("first", "first.txt", "hello"),
        file_part("second", "second.txt", "world"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION).unwrap(),
        "attachment; filename=\"combined.txt\""
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello\nworld");
}

#[tokio::test]
async fn zero_file_parts_returns_wrong_file_count() {
    let app = router(test_state());
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
    let app = router(test_state());
    let body = build_multipart_body(&[file_part("a", "a.txt", "one")]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "wrong_file_count");
}

#[tokio::test]
async fn three_file_parts_returns_wrong_file_count() {
    let app = router(test_state());
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
        file_part("c", "c.txt", "three"),
    ]);
    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(response).await, "wrong_file_count");
}

#[tokio::test]
async fn non_file_field_is_rejected() {
    let app = router(test_state());
    let body = build_multipart_body(&[
        field_part("note", "not a file"),
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
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
    let app = router(test_state());
    let oversized = vec![b'a'; backend::contract::MAX_FILE_BYTES as usize + 1];
    let body = build_multipart_body(&[
        file_part("a", "a.txt", oversized),
        file_part("b", "b.txt", "two"),
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
    let app = router(test_state());
    let body = build_multipart_body(&[
        file_part("a", "a.txt", "one"),
        file_part("b", "b.txt", "two"),
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
    let app = router(test_state());
    let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
    let body = build_multipart_body(&[
        file_part("a", "a.txt", invalid_utf8),
        file_part("b", "b.txt", "two"),
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
    let app = router(test_state());
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

    let app = router(state);
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
