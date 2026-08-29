//! The authenticated `/api/combine` endpoint: bounded, in-memory multipart handling.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use crate::combinator_agent::{CombineAgentBackend, CombineAgentError, CombineRequest};
use crate::contract::{
    ErrorBody, ErrorCode, FIRST_FIELD_NAME, MAX_CONCURRENT_COMBINES, MAX_FILE_BYTES,
    MAX_PART_HEADER_BYTES, MAX_REQUEST_BYTES, REQUEST_TIMEOUT_SECS, SECOND_FIELD_NAME,
};
use crate::diagnostics::{DiagnosticsRecorder, DiagnosticsStore, Level};

/// Header a caller must send, alongside `COMBINE_PROBE_TOKEN`, to use the
/// internal probe path. The browser UI never sends this header.
pub const PROBE_HEADER: &str = "x-combine-probe";
/// Response header carrying the opaque diagnostics capability for a combine
/// request, once one has been issued.
pub const DIAGNOSTICS_CAPABILITY_HEADER: &str = "x-diagnostics-capability";

#[derive(Clone)]
pub struct AppState {
    bearer_token: Arc<str>,
    probe_token: Arc<str>,
    combine_slots: Arc<Semaphore>,
    backend: Arc<dyn CombineAgentBackend>,
    diagnostics: DiagnosticsStore,
}

impl AppState {
    pub fn new(
        bearer_token: String,
        probe_token: String,
        backend: Arc<dyn CombineAgentBackend>,
    ) -> Self {
        AppState {
            bearer_token: bearer_token.into(),
            probe_token: probe_token.into(),
            combine_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_COMBINES)),
            backend,
            diagnostics: DiagnosticsStore::new(),
        }
    }

    /// Exposes the concurrency-limiting semaphore for integration tests that
    /// need to saturate it deterministically.
    pub fn combine_slots(&self) -> &Arc<Semaphore> {
        &self.combine_slots
    }
}

/// A fresh, request-scoped temporary directory. Its contents (staged
/// uploads and any agent-created output) are removed when the guard drops,
/// which covers success, failure, timeout cancellation, and disconnect.
struct RequestDir(PathBuf);

impl RequestDir {
    fn create() -> std::io::Result<Self> {
        let unique = format!(
            "{}-{:?}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::thread::current().id(),
        );
        let dir = std::env::temp_dir().join(format!("file-combiner-{unique}"));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// The directory's unique basename, reused as the diagnostics
    /// correlation id for this request.
    fn id(&self) -> &str {
        self.0
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown-request")
    }
}

impl Drop for RequestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/combine", post(combine))
        .route("/api/diagnostics/:capability", get(diagnostics))
        .with_state(state)
}

/// Returns bounded, redacted diagnostics for one combine request. Requires
/// the same shared bearer as `/api/combine`; an unknown, expired, or
/// already-cleaned capability is indistinguishable from an unknown one.
async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(capability): AxumPath<String>,
) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }

    match state.diagnostics.retrieve(&capability) {
        Some(diagnostics) => (StatusCode::OK, Json(diagnostics)).into_response(),
        None => error_response(ErrorCode::NotFound, "diagnostics not found"),
    }
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

    run_with_deadline(
        permit,
        run_combine(multipart, state.backend.clone(), state.diagnostics.clone()),
    )
    .await
}

/// Runs `op` under the request deadline while holding `permit`. Extracted so
/// tests can inject an operation that never resolves and prove the timeout
/// branch without depending on wall-clock time.
async fn run_with_deadline<Fut>(permit: tokio::sync::OwnedSemaphorePermit, op: Fut) -> Response
where
    Fut: std::future::Future<Output = Response>,
{
    let response = match timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS), op).await {
        Ok(response) => response,
        Err(_) => error_response(ErrorCode::RequestTimeout, "request exceeded the time limit"),
    };
    drop(permit);
    response
}

struct UploadedFile {
    name: String,
    contents: String,
}

async fn run_combine(
    mut multipart: Multipart,
    backend: Arc<dyn CombineAgentBackend>,
    diagnostics: DiagnosticsStore,
) -> Response {
    let mut first_file: Option<UploadedFile> = None;
    let mut second_file: Option<UploadedFile> = None;
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

        let field_name = field.name().unwrap_or_default().to_owned();

        let Some(file_name) = field.file_name().map(str::to_owned) else {
            return error_response(ErrorCode::NonFileField, "only file parts are accepted");
        };

        // Named-field enforcement verifies field names only; callers remain
        // responsible for assigning template and customer content correctly.
        let target = match field_name.as_str() {
            FIRST_FIELD_NAME if first_file.is_none() => &mut first_file,
            SECOND_FIELD_NAME if second_file.is_none() => &mut second_file,
            FIRST_FIELD_NAME | SECOND_FIELD_NAME => {
                return error_response(
                    ErrorCode::InvalidFieldName,
                    "duplicate multipart field name",
                )
            }
            _ => {
                return error_response(
                    ErrorCode::InvalidFieldName,
                    "file field must be named \"first\" or \"second\"",
                )
            }
        };

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
            Ok(contents) => {
                *target = Some(UploadedFile {
                    name: file_name,
                    contents,
                })
            }
            Err(_) => {
                return error_response(ErrorCode::InvalidTextEncoding, "file is not valid UTF-8")
            }
        }
    }

    let (Some(first_file), Some(second_file)) = (first_file, second_file) else {
        return error_response(ErrorCode::WrongFileCount, "exactly two files are required");
    };

    let request_dir = match RequestDir::create() {
        Ok(dir) => dir,
        Err(_) => return error_response(ErrorCode::CombineFailed, "failed to stage the request"),
    };
    let request_id = request_dir.id().to_owned();
    let recorder = DiagnosticsRecorder::new(&request_id);

    recorder.record(
        "combine.received",
        Level::Info,
        format!("received 2 files totaling {total_bytes} bytes"),
    );

    let combine_request = CombineRequest {
        first_name: first_file.name,
        first_contents: first_file.contents,
        second_name: second_file.name,
        second_contents: second_file.contents,
    };

    recorder.record(
        "combine.agent_invoked",
        Level::Info,
        "invoking the combinator agent",
    );

    let combined_path = match backend
        .combine(request_dir.path(), &combine_request, &recorder)
        .await
    {
        Ok(path) => {
            recorder.record(
                "combine.completed",
                Level::Info,
                "combinator agent produced a validated file",
            );
            path
        }
        Err(error) => {
            let message = match error {
                CombineAgentError::Execution(_) => "the combine agent failed to run",
                CombineAgentError::NoFileCreated => "the combine agent did not produce a file",
                CombineAgentError::InvalidOutput(_) => "the combine agent produced an invalid file",
            };
            // The raw detail may reference paths/config, so it goes only to
            // the operator-facing tracing log — never into the redacted,
            // client-readable diagnostics capability or HTTP response.
            tracing::error!(request_id = %request_id, "combine agent failed: {error}");
            recorder.record("combine.failed", Level::Error, message);
            let capability = diagnostics.issue(&request_id, recorder.into_events());
            let mut response = error_response(ErrorCode::CombineFailed, message);
            insert_capability_header(&mut response, &capability);
            return response;
        }
    };

    let combined = match fs::read(&combined_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            recorder.record(
                "combine.failed",
                Level::Error,
                "failed to read the combined file",
            );
            let capability = diagnostics.issue(&request_id, recorder.into_events());
            let mut response =
                error_response(ErrorCode::CombineFailed, "failed to read combined file");
            insert_capability_header(&mut response, &capability);
            return response;
        }
    };
    // `request_dir` drops here, deleting staged uploads and the created
    // file now that its bytes are already read into the response body.
    drop(request_dir);

    recorder.record(
        "combine.response_returned",
        Level::Info,
        "returning the combined file to the caller",
    );
    let capability = diagnostics.issue(&request_id, recorder.into_events());

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"combined.txt\"",
        )
        .body(Body::from(combined))
        .expect("response with well-formed static headers");
    insert_capability_header(&mut response, &capability);
    response
}

fn insert_capability_header(response: &mut Response, capability: &str) {
    if let Ok(value) = capability.parse() {
        response.headers_mut().insert(
            HeaderName::from_static(DIAGNOSTICS_CAPABILITY_HEADER),
            value,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinator_agent::FakeCombineBackend;

    fn test_state() -> AppState {
        AppState::new(
            "token".to_string(),
            "probe".to_string(),
            Arc::new(FakeCombineBackend),
        )
    }

    /// Exercises the actual timeout branch: an operation that never
    /// resolves must be aborted once the deadline elapses, independent of
    /// wall-clock time.
    #[tokio::test(start_paused = true)]
    async fn deadline_returns_request_timeout_response() {
        let state = test_state();
        let permit = state
            .combine_slots
            .clone()
            .try_acquire_owned()
            .expect("fresh semaphore has capacity");

        let handle = tokio::spawn(run_with_deadline(permit, std::future::pending()));
        tokio::time::advance(Duration::from_secs(REQUEST_TIMEOUT_SECS + 1)).await;

        let response = handle.await.expect("task completes");
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    }
}
