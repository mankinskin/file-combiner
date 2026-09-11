//! The `/api/analyze` endpoint: multipart parsing, backend invocation, and
//! analysis-session completion.
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::combinator_agent::{AnalysisKind, CombineAgentError, LatentRepresentation};
use crate::contract::{ErrorCode, MAX_REQUEST_BYTES};
use crate::diagnostics::{DiagnosticsRecorder, Level};

use super::auth::{authorize, AuthorizationResult};
use super::handler::{error_response, error_response_with_details};
use super::session_store::AnalysisResponse;
use super::AppState;

/// The fields collected from one `/api/analyze` multipart request.
struct ParsedAnalysisRequest {
    kind: AnalysisKind,
    session_id: Option<String>,
    name: String,
    contents: String,
}

/// Consumes the analyze multipart body into its constituent fields,
/// enforcing the size and field-name rules the endpoint requires.
async fn parse_analysis_multipart(
    mut multipart: Multipart,
) -> Result<ParsedAnalysisRequest, Response> {
    let mut kind = None;
    let mut session_id = None;
    let mut uploaded = None;
    let mut total_bytes = 0u64;
    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return Err(error_response(
                    ErrorCode::MalformedMultipart,
                    "malformed multipart body",
                ))
            }
        };
        let field_name = field.name().unwrap_or_default().to_owned();
        match field_name.as_str() {
            "kind" => kind = Some(parse_analysis_kind(field.text().await.ok().as_deref())?),
            "session_id" => session_id = field.text().await.ok().filter(|id| !id.is_empty()),
            "artifact" => {
                uploaded = Some(read_analysis_artifact(&mut field, &mut total_bytes).await?)
            }
            _ => {
                return Err(error_response(
                    ErrorCode::InvalidFieldName,
                    "unknown analysis field",
                ))
            }
        }
    }
    let Some(kind) = kind else {
        return Err(error_response(
            ErrorCode::InvalidFieldName,
            "analysis kind is required",
        ));
    };
    let Some((name, contents)) = uploaded else {
        return Err(error_response(
            ErrorCode::WrongFileCount,
            "one artifact is required",
        ));
    };
    Ok(ParsedAnalysisRequest {
        kind,
        session_id,
        name,
        contents,
    })
}

fn parse_analysis_kind(raw: Option<&str>) -> Result<AnalysisKind, Response> {
    match raw {
        Some("template") => Ok(AnalysisKind::Template),
        Some("profile") | Some("person-profile") => Ok(AnalysisKind::PersonProfile),
        _ => Err(error_response(
            ErrorCode::InvalidFieldName,
            "kind must be template or profile",
        )),
    }
}

/// Reads the `artifact` field's body, enforcing both the per-file and
/// running-total request byte caps before decoding it as UTF-8.
async fn read_analysis_artifact(
    field: &mut axum::extract::multipart::Field<'_>,
    total_bytes: &mut u64,
) -> Result<(String, String), Response> {
    let name = field.file_name().unwrap_or("artifact.txt").to_owned();
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = field.chunk().await {
        *total_bytes += chunk.len() as u64;
        if *total_bytes > MAX_REQUEST_BYTES
            || bytes.len() + chunk.len() > crate::contract::MAX_FILE_BYTES as usize
        {
            return Err(error_response(
                ErrorCode::PayloadTooLarge,
                "artifact exceeds the size limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    match String::from_utf8(bytes) {
        Ok(contents) => Ok((name, contents)),
        Err(_) => Err(error_response(
            ErrorCode::InvalidTextEncoding,
            "artifact is not valid UTF-8",
        )),
    }
}

/// Runs the analysis backend and maps a backend failure to the diagnostics
/// capability and error response the caller receives. Returns the
/// recorder back to the caller so its events accumulate across the request.
async fn invoke_analyze_backend<'a>(
    state: &AppState,
    recorder: DiagnosticsRecorder<'a>,
    request_id: &str,
    kind: AnalysisKind,
    name: &str,
    contents: &str,
    direct_pat: Option<&str>,
) -> Result<(LatentRepresentation, DiagnosticsRecorder<'a>), Response> {
    match state
        .backend()
        .analyze(kind, name, contents, &recorder, direct_pat)
        .await
    {
        Ok(representation) => Ok((representation, recorder)),
        Err(error) => {
            recorder.record(
                "analyze.backend_invocation.failed",
                Level::Error,
                format!("the configured analysis backend failed: {error}"),
            );
            let capability = state.diagnostics.issue(request_id, recorder.into_events());
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
            Err(response)
        }
    }
}

/// When the analysis session just became complete with nothing generated
/// yet, invokes the latent-combination backend and stores its result.
/// Otherwise passes `response` through unchanged.
async fn complete_analysis_if_ready<'a>(
    state: &AppState,
    recorder: DiagnosticsRecorder<'a>,
    request_id: &str,
    response: AnalysisResponse,
    direct_pat: Option<&str>,
) -> Result<(AnalysisResponse, DiagnosticsRecorder<'a>), Response> {
    if !(response.complete && response.artifacts.is_empty()) {
        return Ok((response, recorder));
    }
    let Some((template, profile)) = state.analysis_sessions().inputs(&response.session_id) else {
        return Err(error_response(
            ErrorCode::NotFound,
            "analysis session not found",
        ));
    };
    match state
        .backend()
        .combine_latent(&template, &profile, &recorder, direct_pat)
        .await
    {
        Ok(generated) => match state
            .analysis_sessions()
            .store_generated(&response.session_id, generated)
        {
            Ok(response) => Ok((response, recorder)),
            Err(message) => Err(error_response(ErrorCode::ServiceBusy, message)),
        },
        Err(error) => {
            recorder.record(
                "analyze.completion_backend.failed",
                Level::Error,
                format!("the configured completion backend failed: {error}"),
            );
            let capability = state.diagnostics.issue(request_id, recorder.into_events());
            let code = match error {
                CombineAgentError::ModelUnauthenticated(_) => ErrorCode::ModelUnauthenticated,
                _ => ErrorCode::CombineFailed,
            };
            let mut response = error_response_with_details(
                code,
                "Artifact generation failed while invoking the configured model backend. See details and diagnostics for the backend error.",
                [
                    format!("Backend error: {error}"),
                    format!("Diagnostics capability: {capability}"),
                ],
            );
            super::pipeline::insert_capability_header(&mut response, &capability);
            Err(response)
        }
    }
}

/// Runs the analysis backend, validates a supplied session id still exists,
/// and stores the resulting representation. Collapses what would otherwise
/// be three separate call sites in `analyze` into one.
async fn run_analysis<'a>(
    state: &AppState,
    recorder: DiagnosticsRecorder<'a>,
    request_id: &str,
    parsed: &ParsedAnalysisRequest,
    direct_pat: Option<&str>,
) -> Result<(AnalysisResponse, DiagnosticsRecorder<'a>), Response> {
    let (representation, recorder) = invoke_analyze_backend(
        state,
        recorder,
        request_id,
        parsed.kind,
        &parsed.name,
        &parsed.contents,
        direct_pat,
    )
    .await?;

    if let Some(session_id) = parsed.session_id.as_deref() {
        if !state.analysis_sessions().contains(session_id) {
            return Err(error_response(
                ErrorCode::NotFound,
                "analysis session not found",
            ));
        }
    }

    let response = state
        .analysis_sessions()
        .upsert(parsed.session_id.as_deref(), representation)
        .map_err(|message| error_response(ErrorCode::ServiceBusy, message))?;

    Ok((response, recorder))
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
    let multipart = match multipart {
        Ok(multipart) => multipart,
        Err(_) => return error_response(ErrorCode::MalformedMultipart, "malformed multipart body"),
    };

    let parsed = match parse_analysis_multipart(multipart).await {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };

    let request_id = parsed
        .session_id
        .clone()
        .unwrap_or_else(|| "analysis".to_owned());
    let recorder = DiagnosticsRecorder::new(&request_id);
    let (response, recorder) = match run_analysis(
        &state,
        recorder,
        &request_id,
        &parsed,
        direct_pat.as_deref(),
    )
    .await
    {
        Ok(pair) => pair,
        Err(response) => return response,
    };

    let (response, recorder) = match complete_analysis_if_ready(
        &state,
        recorder,
        &request_id,
        response,
        direct_pat.as_deref(),
    )
    .await
    {
        Ok(pair) => pair,
        Err(response) => return response,
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
