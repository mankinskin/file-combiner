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
    assert!(obj.contains_key("code"));
    assert!(obj.contains_key("message"));
    assert!(obj.contains_key("status"));
    assert!(obj.contains_key("details"));
    assert!(
        !obj.get("details").unwrap().as_array().unwrap().is_empty(),
        "error body must include descriptive details"
    );
    obj.get("code").unwrap().as_str().unwrap().to_string()
}

#[path = "combine/artifacts.rs"]
mod artifacts;
#[path = "combine/auth.rs"]
mod auth;
#[path = "combine/diagnostics.rs"]
mod diagnostics;
#[path = "combine/multipart.rs"]
mod multipart;
