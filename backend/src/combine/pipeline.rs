//! The authenticated combine flow: stages the two uploaded files (via
//! [`super::multipart`]), invokes the combinator backend, and retains the
//! resulting artifacts.
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Multipart;
use axum::http::{header, HeaderName, StatusCode};
use axum::response::Response;

use crate::combinator_agent::{
    CombineAgentBackend, CombineAgentError, CombineRequest, GeneratedArtifact,
};
use crate::contract::ErrorCode;
use crate::diagnostics::{DiagnosticsRecorder, DiagnosticsStore, Level};

use super::artifact_store::{ArtifactListResponse, ArtifactResponse, ArtifactStore};
use super::handler::{error_response, error_response_with_details};
use super::multipart::{parse_combine_multipart, RequestDir};
use super::DIAGNOSTICS_CAPABILITY_HEADER;

/// Invokes the combinator backend and maps a failure to the diagnostics
/// capability and error response the caller receives. Returns the
/// recorder back to the caller so its events accumulate across the request.
async fn invoke_combine_backend<'a>(
    backend: &dyn CombineAgentBackend,
    dir: &Path,
    combine_request: &CombineRequest,
    recorder: DiagnosticsRecorder<'a>,
    request_id: &str,
    diagnostics: &DiagnosticsStore,
    direct_pat: Option<&str>,
) -> Result<(Vec<GeneratedArtifact>, DiagnosticsRecorder<'a>), Response> {
    match backend
        .combine(dir, combine_request, &recorder, direct_pat)
        .await
    {
        Ok(path) => {
            recorder.record(
                "combine.completed",
                Level::Info,
                "combinator agent produced a validated file",
            );
            Ok((path, recorder))
        }
        Err(error) => {
            let message = match &error {
                CombineAgentError::InvalidTemplate(detail) => {
                    format!("the template contract is invalid: {detail}")
                }
                CombineAgentError::Execution(_) => {
                    "Artifact generation failed while invoking the configured model backend. See details and diagnostics for the backend error.".to_owned()
                }
                CombineAgentError::ModelUnauthenticated(_) => {
                    "Copilot authentication is not configured on the server. Set GITHUB_COPILOT_TOKEN or COPILOT_API_KEY.".to_owned()
                }
                CombineAgentError::NoFileCreated => {
                    "the combine agent did not produce a file".to_owned()
                }
                CombineAgentError::InvalidOutput(_) => {
                    "the combine agent produced an invalid file".to_owned()
                }
            };
            tracing::error!(request_id = %request_id, "combine agent failed: {error}");
            recorder.record(
                "combine.agent_invocation.failed",
                Level::Error,
                format!("attempted to generate artifacts with the configured combine backend; failed: {error}"),
            );
            let capability = diagnostics.issue(request_id, recorder.into_events());
            let code = match error {
                CombineAgentError::ModelUnauthenticated(_) => ErrorCode::ModelUnauthenticated,
                _ => ErrorCode::CombineFailed,
            };
            let mut response = error_response_with_details(
                code,
                &message,
                [
                    format!("Backend error: {error}"),
                    format!("Diagnostics capability: {capability}"),
                ],
            );
            insert_capability_header(&mut response, &capability);
            Err(response)
        }
    }
}

/// Retains the generated artifact bytes for later authenticated preview and
/// download, mapping a storage failure to the diagnostics capability and
/// error response the caller receives.
fn store_combine_artifacts<'a>(
    artifacts: &ArtifactStore,
    request_id: &str,
    generated_artifacts: Vec<GeneratedArtifact>,
    recorder: DiagnosticsRecorder<'a>,
    diagnostics: &DiagnosticsStore,
) -> Result<(Vec<ArtifactResponse>, DiagnosticsRecorder<'a>), Response> {
    recorder.record(
        "combine.artifact_retention.attempted",
        Level::Info,
        "attempting to retain generated artifact bytes for authenticated previews and downloads",
    );
    match artifacts.insert(request_id, generated_artifacts) {
        Ok(artifact_list) => {
            recorder.record(
                "combine.artifact_retention.succeeded",
                Level::Info,
                format!(
                    "retained {} generated artifacts for five minutes",
                    artifact_list.len()
                ),
            );
            Ok((artifact_list, recorder))
        }
        Err(error) => {
            recorder.record(
                "combine.artifact_retention.failed",
                Level::Error,
                format!("attempted to retain generated artifacts; failed: {error}"),
            );
            let capability = diagnostics.issue(request_id, recorder.into_events());
            let mut response = error_response_with_details(
                ErrorCode::CombineFailed,
                "failed to retain generated artifacts",
                [
                    format!("Artifact retention error: {error}"),
                    format!("Diagnostics capability: {capability}"),
                ],
            );
            insert_capability_header(&mut response, &capability);
            Err(response)
        }
    }
}

pub(super) async fn run_combine(
    mut multipart: Multipart,
    backend: Arc<dyn CombineAgentBackend>,
    diagnostics: DiagnosticsStore,
    artifacts: ArtifactStore,
    direct_pat: Option<String>,
) -> Response {
    let (first_file, second_file, total_bytes) = match parse_combine_multipart(&mut multipart).await
    {
        Ok(parsed) => parsed,
        Err(response) => return response,
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

    let (generated_artifacts, recorder) = match invoke_combine_backend(
        backend.as_ref(),
        request_dir.path(),
        &combine_request,
        recorder,
        &request_id,
        &diagnostics,
        direct_pat.as_deref(),
    )
    .await
    {
        Ok(pair) => pair,
        Err(response) => return response,
    };

    let (artifact_list, recorder) = match store_combine_artifacts(
        &artifacts,
        &request_id,
        generated_artifacts,
        recorder,
        &diagnostics,
    ) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    // `request_dir` drops here after the output bytes enter the bounded store.
    drop(request_dir);

    recorder.record(
        "combine.response_delivery.succeeded",
        Level::Info,
        "returning the generated artifact manifest and diagnostics capability to the caller",
    );
    let capability = diagnostics.issue(&request_id, recorder.into_events());

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&ArtifactListResponse {
                artifacts: artifact_list,
            })
            .expect("artifact manifest is serializable"),
        ))
        .expect("response with well-formed static headers");
    insert_capability_header(&mut response, &capability);
    response
}

pub(super) fn insert_capability_header(response: &mut Response, capability: &str) {
    if let Ok(value) = capability.parse() {
        response.headers_mut().insert(
            HeaderName::from_static(DIAGNOSTICS_CAPABILITY_HEADER),
            value,
        );
    }
}
