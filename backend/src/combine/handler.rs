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

use crate::contract::{ErrorBody, ErrorCode, MAX_REQUEST_BYTES, REQUEST_TIMEOUT_SECS};

use super::auth::{authorize, AuthorizationResult};
use super::pipeline::run_combine;
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

/// Returns bounded, redacted diagnostics for one combine request. Requires
/// the same shared bearer as `/api/combine`; an unknown, expired, or
/// already-cleaned capability is indistinguishable from an unknown one.
pub(super) async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(capability): AxumPath<String>,
) -> Response {
    // Diagnostics requires the configured bearer token, not direct PATs
    match authorize(&state, &headers) {
        AuthorizationResult::Unauthorized(response) => return response,
        AuthorizationResult::Authorized => {}
        AuthorizationResult::AuthorizedWithDirectPat(_) => {
            return error_response(
                ErrorCode::Unauthorized,
                "diagnostics requires bearer token, not direct PAT",
            )
        }
    }

    match state.diagnostics.retrieve(&capability) {
        Some(diagnostics) => (StatusCode::OK, Json(diagnostics)).into_response(),
        None => error_response(ErrorCode::NotFound, "diagnostics not found"),
    }
}

pub(super) fn error_response(code: ErrorCode, message: &str) -> Response {
    let status = StatusCode::from_u16(code.status()).expect("valid status code");
    (status, Json(ErrorBody::new(code, message))).into_response()
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
