//! Route handlers for the combine, download, and diagnostics endpoints,
//! plus the shared error-response helper and request-deadline wrapper.
use std::time::Duration;

use axum::body::Body;
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use tokio::time::timeout;

use crate::combinator_agent::{AnalysisKind, CombineAgentError};
use crate::contract::{ErrorBody, ErrorCode, MAX_REQUEST_BYTES, REQUEST_TIMEOUT_SECS};

use super::auth::{authorize, AuthorizationResult};
use super::pipeline::run_combine;
use super::session_store::AnalysisResponse;
use super::AppState;

pub(super) async fn download_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((request_id, artifact_id)): AxumPath<(String, String)>,
) -> Response {
    match authorize(&state, &headers) {
        AuthorizationResult::Unauthorized(response) => return response,
        AuthorizationResult::Authorized | AuthorizationResult::AuthorizedWithDirectPat(_) => {}
    }

    let Some(content) = state.artifacts.get(&request_id, &artifact_id) else {
        return error_response(ErrorCode::NotFound, "artifact not found");
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{artifact_id}.txt\""),
        )
        .body(Body::from(content))
        .expect("response with well-formed static headers")
}

pub(super) async fn download_session_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((session_id, artifact_id)): AxumPath<(String, String)>,
) -> Response {
    match authorize(&state, &headers) {
        AuthorizationResult::Unauthorized(response) => return response,
        AuthorizationResult::Authorized | AuthorizationResult::AuthorizedWithDirectPat(_) => {}
    }
    let Some(artifact) = state
        .analysis_sessions()
        .get_artifact(&session_id, &artifact_id)
    else {
        return error_response(ErrorCode::NotFound, "artifact not found");
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.md\"", artifact.name),
        )
        .body(Body::from(artifact.content))
        .expect("response with well-formed static headers")
}

/// Returns bounded, redacted diagnostics for one combine request. Requires
/// the same shared bearer as `/api/combine`; an unknown, expired, or
/// already-cleaned capability is indistinguishable from an unknown one.
pub(super) async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(capability): AxumPath<String>,
) -> Response {
    // Diagnostics capabilities are opaque, request-scoped bearer tokens.
    // Direct PAT callers need this path too, otherwise analysis failures hide
    // the backend trace from the authentication mode that produced them.
    match authorize(&state, &headers) {
        AuthorizationResult::Unauthorized(response) => return response,
        AuthorizationResult::Authorized => {}
        AuthorizationResult::AuthorizedWithDirectPat(_) => {}
    }

    match state.diagnostics.retrieve(&capability) {
        Some(diagnostics) => (StatusCode::OK, Json(diagnostics)).into_response(),
        None => error_response(ErrorCode::NotFound, "diagnostics not found"),
    }
}

pub(super) async fn analyze(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    let auth_result = authorize(&state, &headers);
    let direct_pat = match auth_result {
        AuthorizationResult::Unauthorized(response) => return response,
        AuthorizationResult::Authorized => None,
        AuthorizationResult::AuthorizedWithDirectPat(token) => Some(token),
    };
    let mut multipart = match multipart {
        Ok(multipart) => multipart,
        Err(_) => return error_response(ErrorCode::MalformedMultipart, "malformed multipart body"),
    };
    let mut kind = None;
    let mut session_id = None;
    let mut uploaded = None;
    let mut total_bytes = 0u64;
    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return error_response(ErrorCode::MalformedMultipart, "malformed multipart body")
            }
        };
        let field_name = field.name().unwrap_or_default().to_owned();
        match field_name.as_str() {
            "kind" => {
                kind = match field.text().await.ok().as_deref() {
                    Some("template") => Some(AnalysisKind::Template),
                    Some("profile") | Some("person-profile") => Some(AnalysisKind::PersonProfile),
                    _ => {
                        return error_response(
                            ErrorCode::InvalidFieldName,
                            "kind must be template or profile",
                        )
                    }
                };
            }
            "session_id" => session_id = field.text().await.ok().filter(|id| !id.is_empty()),
            "artifact" => {
                let name = field.file_name().unwrap_or("artifact.txt").to_owned();
                let mut bytes = Vec::new();
                while let Ok(Some(chunk)) = field.chunk().await {
                    total_bytes += chunk.len() as u64;
                    if total_bytes > MAX_REQUEST_BYTES
                        || bytes.len() + chunk.len() > crate::contract::MAX_FILE_BYTES as usize
                    {
                        return error_response(
                            ErrorCode::PayloadTooLarge,
                            "artifact exceeds the size limit",
                        );
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let contents = match String::from_utf8(bytes) {
                    Ok(contents) => contents,
                    Err(_) => {
                        return error_response(
                            ErrorCode::InvalidTextEncoding,
                            "artifact is not valid UTF-8",
                        )
                    }
                };
                uploaded = Some((name, contents));
            }
            _ => return error_response(ErrorCode::InvalidFieldName, "unknown analysis field"),
        }
    }
    let Some(kind) = kind else {
        return error_response(ErrorCode::InvalidFieldName, "analysis kind is required");
    };
    let Some((name, contents)) = uploaded else {
        return error_response(ErrorCode::WrongFileCount, "one artifact is required");
    };
    let request_id = session_id.clone().unwrap_or_else(|| "analysis".to_owned());
    let recorder = crate::diagnostics::DiagnosticsRecorder::new(&request_id);
    let representation = match state
        .backend()
        .analyze(kind, &name, &contents, &recorder, direct_pat.as_deref())
        .await
    {
        Ok(representation) => representation,
        Err(error) => {
            recorder.record(
                "analyze.backend_invocation.failed",
                crate::diagnostics::Level::Error,
                format!("the configured analysis backend failed: {error}"),
            );
            let capability = state.diagnostics.issue(&request_id, recorder.into_events());
            let (code, message) = match &error {
                CombineAgentError::ModelUnauthenticated(_) => (
                    ErrorCode::ModelUnauthenticated,
                    "Copilot authentication is not configured on the server. Set GITHUB_COPILOT_TOKEN or COPILOT_API_KEY.",
                ),
                _ => (
                    ErrorCode::CombineFailed,
                    "Artifact analysis failed while invoking the configured model backend. See details and diagnostics for the backend error.",
                ),
            };
            let mut response = error_response_with_details(
                code,
                message,
                [
                    format!("Backend error: {error}"),
                    format!("Diagnostics capability: {capability}"),
                ],
            );
            super::pipeline::insert_capability_header(&mut response, &capability);
            return response;
        }
    };
    if let Some(session_id) = session_id.as_deref() {
        if !state.analysis_sessions().contains(session_id) {
            return error_response(ErrorCode::NotFound, "analysis session not found");
        }
    }
    let response = match state
        .analysis_sessions()
        .upsert(session_id.as_deref(), representation)
    {
        Ok(response) => response,
        Err(message) => return error_response(ErrorCode::ServiceBusy, message),
    };
    let capability = state.diagnostics.issue(&request_id, recorder.into_events());
    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Json::<AnalysisResponse>(response),
    )
        .into_response();
    super::pipeline::insert_capability_header(&mut response, &capability);
    response
}

pub(super) fn error_response(code: ErrorCode, message: &str) -> Response {
    let status = StatusCode::from_u16(code.status()).expect("valid status code");
    (status, Json(ErrorBody::new(code, message))).into_response()
}

pub(super) fn error_response_with_details<I, S>(
    code: ErrorCode,
    message: &str,
    details: I,
) -> Response
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let status = StatusCode::from_u16(code.status()).expect("valid status code");
    (
        status,
        Json(ErrorBody::with_details(code, message, details)),
    )
        .into_response()
}

pub(super) async fn combine(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    let auth_result = authorize(&state, &headers);
    let direct_pat = match auth_result {
        AuthorizationResult::Unauthorized(response) => return response,
        AuthorizationResult::Authorized => None,
        AuthorizationResult::AuthorizedWithDirectPat(token) => Some(token),
    };

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
        run_combine(
            multipart,
            state.backend.clone(),
            state.diagnostics.clone(),
            state.artifacts.clone(),
            direct_pat,
        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinator_agent::FakeCombineBackend;
    use std::sync::Arc;

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
