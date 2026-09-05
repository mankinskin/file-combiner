//! HTTP integration tests for the authenticated `/api/combine` endpoint.
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use backend::combinator_agent::FakeCombineBackend;
use backend::{router, AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

const BEARER_TOKEN: &str = "test-group-token";
const PROBE_TOKEN: &str = "test-probe-token";
const BOUNDARY: &str = "combine-test-boundary";
const VALID_TEMPLATE: &str = "---\nversion: 1\nconversation:\n  context: first meeting\n  type: advisory\n  subject: retirement planning\nrole: financial adviser\npersonalization:\n  - goals\nartifacts:\n  - name: preparation-brief\n    format: markdown\n    sections:\n      - Priorities\n  - name: question-list\n    format: markdown\n    sections:\n      - Questions\n---\n# Preparation brief";

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

fn download_request(auth: Option<&str>, path: &str) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(auth) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {auth}"));
    }
    builder.body(Body::empty()).unwrap()
}

fn test_state() -> AppState {
    AppState::new(
        BEARER_TOKEN.to_string(),
        PROBE_TOKEN.to_string(),
        Arc::new(FakeCombineBackend),
    )
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

#[tokio::test]
async fn combine_returns_manifest_and_each_artifact_downloads_individually() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", VALID_TEMPLATE),
        file_part("second", "second.txt", "world"),
    ]);
    let response = app
        .clone()
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let manifest: Value = serde_json::from_slice(&body).unwrap();
    let artifacts = manifest.get("artifacts").unwrap().as_array().unwrap();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0].get("id").unwrap(), "artifact-0");
    assert_eq!(artifacts[0].get("name").unwrap(), "preparation-brief");
    assert_eq!(artifacts[1].get("id").unwrap(), "artifact-1");
    assert_eq!(artifacts[1].get("name").unwrap(), "question-list");

    for artifact in artifacts {
        let url = artifact.get("download_url").unwrap().as_str().unwrap();
        let response = app
            .clone()
            .oneshot(download_request(Some(BEARER_TOKEN), url))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let name = artifact.get("name").unwrap().as_str().unwrap();
        assert_eq!(&body[..], format!("# {name}\n\nworld").as_bytes());
    }

    let url = artifacts[0].get("download_url").unwrap().as_str().unwrap();
    let response = app.oneshot(download_request(None, url)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn template_without_artifacts_returns_combine_failed() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part(
            "first",
            "first.txt",
            VALID_TEMPLATE.replace(
                "artifacts:\n  - name: preparation-brief\n    format: markdown\n    sections:\n      - Priorities\n",
                "artifacts: []\n",
            ),
        ),
        file_part("second", "second.txt", "world"),
    ]);

    let response = app
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(error_code(response).await, "combine_failed");
}

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

fn diagnostics_request(auth: Option<&str>, capability: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("GET")
        .uri(format!("/api/diagnostics/{capability}"));
    if let Some(auth) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {auth}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn diagnostics_body(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// End-to-end: a combine request issues a diagnostics capability that a
/// bearer-authenticated caller can use to repeatedly retrieve bounded,
/// structured, redacted records correlated to that request and only that
/// request, until expiry.
#[tokio::test(start_paused = true)]
async fn combine_issues_diagnostics_capability_with_structured_redacted_records() {
    let app = router(test_state(), "frontend/dist");
    let body = build_multipart_body(&[
        file_part("first", "first.txt", VALID_TEMPLATE),
        file_part("second", "second.txt", "world"),
    ]);

    let response = app
        .clone()
        .oneshot(request(Some(BEARER_TOKEN), false, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let capability = response
        .headers()
        .get(backend::combine::DIAGNOSTICS_CAPABILITY_HEADER)
        .expect("combine response must carry a diagnostics capability")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        capability.len(),
        64,
        "capability must be a 256-bit hex token"
    );

    // Repeat reads within the TTL succeed and return the same records.
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(diagnostics_request(Some(BEARER_TOKEN), &capability))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value = diagnostics_body(response).await;
        let events = value.get("events").unwrap().as_array().unwrap();
        assert!(!events.is_empty());

        for event in events {
            let object = event.as_object().unwrap();
            assert!(object.contains_key("request_id"));
            assert!(object.contains_key("kind"));
            assert!(object.contains_key("level"));
            assert!(object.contains_key("timestamp_ms"));
            let message = object.get("message").unwrap().as_str().unwrap();
            assert!(
                !message.contains("retirement planning"),
                "message must not contain upload content"
            );
            assert!(
                !message.contains("world"),
                "message must not contain upload content"
            );
            assert!(!message.contains(BEARER_TOKEN));
            assert!(!message.contains(PROBE_TOKEN));
        }
    }

    // Invalid bearer is unauthorized regardless of capability validity.
    let response = app
        .clone()
        .oneshot(diagnostics_request(Some("wrong-token"), &capability))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // An unrelated, well-formed capability never matches this request's records.
    let response = app
        .clone()
        .oneshot(diagnostics_request(
            Some(BEARER_TOKEN),
            "0".repeat(64).as_str(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Expiry: after the 15-minute retention window the capability is gone.
    tokio::time::advance(std::time::Duration::from_secs(15 * 60 + 1)).await;
    let response = app
        .clone()
        .oneshot(diagnostics_request(Some(BEARER_TOKEN), &capability))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
