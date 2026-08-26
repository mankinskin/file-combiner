//! The authenticated `/api/combine` endpoint: bounded, in-memory multipart handling.
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use crate::contract::{
    ErrorBody, ErrorCode, MAX_CONCURRENT_COMBINES, MAX_FILE_BYTES, MAX_PART_HEADER_BYTES,
    MAX_REQUEST_BYTES, REQUEST_TIMEOUT_SECS, REQUIRED_FILE_PARTS,
};

/// Header a caller must send, alongside `COMBINE_PROBE_TOKEN`, to use the
/// internal probe path. The browser UI never sends this header.
pub const PROBE_HEADER: &str = "x-combine-probe";

#[derive(Clone)]
pub struct AppState {
    bearer_token: Arc<str>,
    probe_token: Arc<str>,
    combine_slots: Arc<Semaphore>,
}

impl AppState {
    pub fn new(bearer_token: String, probe_token: String) -> Self {
        AppState {
            bearer_token: bearer_token.into(),
            probe_token: probe_token.into(),
            combine_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_COMBINES)),
        }
    }

    /// Exposes the concurrency-limiting semaphore for integration tests that
    /// need to saturate it deterministically.
    pub fn combine_slots(&self) -> &Arc<Semaphore> {
        &self.combine_slots
    }
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/combine", post(combine))
        .with_state(state)
}

fn error_response(code: ErrorCode, message: &str) -> Response {
    let status = StatusCode::from_u16(code.status()).expect("valid status code");
    (status, Json(ErrorBody::new(code, message))).into_response()
}

/// Checks the bearer credential before any file content is processed.
/// `COMBINE_BEARER_TOKEN` always authorizes; `COMBINE_PROBE_TOKEN` only
/// authorizes when the internal probe header is also present. Returns the
/// rejection response when unauthorized, `None` otherwise.
fn authorize(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let Some(token) = token else {
        return Some(error_response(
            ErrorCode::Unauthorized,
            "missing credentials",
        ));
    };

    if token == state.bearer_token.as_ref() {
        return None;
    }

    if token == state.probe_token.as_ref() && headers.contains_key(PROBE_HEADER) {
        return None;
    }

    Some(error_response(
        ErrorCode::Unauthorized,
        "invalid credentials",
    ))
}

async fn combine(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }

    // The raw body limit is evaluated before multipart parsing begins.
    if let Some(content_length) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        if content_length > MAX_REQUEST_BYTES {
            return error_response(ErrorCode::PayloadTooLarge, "request body too large");
        }
    }

    let multipart = match multipart {
        Ok(multipart) => multipart,
        Err(_) => return error_response(ErrorCode::MalformedMultipart, "malformed multipart body"),
    };

    let permit = match state.combine_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return error_response(ErrorCode::ServiceBusy, "too many concurrent requests"),
    };

    let response = match timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECS),
        run_combine(multipart),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => error_response(ErrorCode::RequestTimeout, "request exceeded the time limit"),
    };
    drop(permit);
    response
}

async fn run_combine(mut multipart: Multipart) -> Response {
    let mut files: Vec<String> = Vec::with_capacity(REQUIRED_FILE_PARTS);
    let mut total_bytes: u64 = 0;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return error_response(ErrorCode::MalformedMultipart, "malformed multipart body")
            }
        };

        let metadata_len = field.name().unwrap_or_default().len()
            + field.file_name().unwrap_or_default().len()
            + field.content_type().unwrap_or_default().len();
        if metadata_len > MAX_PART_HEADER_BYTES {
            return error_response(ErrorCode::MetadataTooLarge, "part metadata too large");
        }

        if field.file_name().is_none() {
            return error_response(ErrorCode::NonFileField, "only file parts are accepted");
        }

        if files.len() >= REQUIRED_FILE_PARTS {
            return error_response(ErrorCode::WrongFileCount, "exactly two files are required");
        }

        let mut field = field;
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    buffer.extend_from_slice(&chunk);
                    if buffer.len() as u64 > MAX_FILE_BYTES {
                        return error_response(
                            ErrorCode::FileTooLarge,
                            "file exceeds the size limit",
                        );
                    }
                    total_bytes += chunk.len() as u64;
                    if total_bytes > MAX_REQUEST_BYTES {
                        return error_response(
                            ErrorCode::PayloadTooLarge,
                            "request body too large",
                        );
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    return error_response(
                        ErrorCode::MalformedMultipart,
                        "malformed multipart body",
                    )
                }
            }
        }

        match String::from_utf8(buffer) {
            Ok(text) => files.push(text),
            Err(_) => {
                return error_response(ErrorCode::InvalidTextEncoding, "file is not valid UTF-8")
            }
        }
    }

    if files.len() != REQUIRED_FILE_PARTS {
        return error_response(ErrorCode::WrongFileCount, "exactly two files are required");
    }

    let combined = format!("{}\n{}", files[0], files[1]);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"combined.txt\"",
        )
        .body(Body::from(combined))
        .expect("response with well-formed static headers")
}
