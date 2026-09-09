//! Bounded multipart parsing and the authenticated combine flow: stages the
//! two uploaded files, invokes the combinator backend, and retains the
//! resulting artifacts.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Multipart;
use axum::http::{header, HeaderName, StatusCode};
use axum::response::Response;

use crate::combinator_agent::{CombineAgentBackend, CombineAgentError, CombineRequest};
use crate::contract::{
    ErrorCode, FIRST_FIELD_NAME, MAX_FILE_BYTES, MAX_PART_HEADER_BYTES, MAX_REQUEST_BYTES,
    SECOND_FIELD_NAME,
};
use crate::diagnostics::{DiagnosticsRecorder, DiagnosticsStore, Level};

use super::artifact_store::{ArtifactListResponse, ArtifactStore};
use super::handler::{error_response, error_response_with_details};
use super::DIAGNOSTICS_CAPABILITY_HEADER;

/// A fresh, request-scoped temporary directory. Its contents (staged
/// uploads and any agent-created output) are removed when the guard drops,
/// which covers success, failure, timeout cancellation, and disconnect.
pub(super) struct RequestDir(PathBuf);

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

struct UploadedFile {
    name: String,
    contents: String,
}

pub(super) async fn run_combine(
    mut multipart: Multipart,
    backend: Arc<dyn CombineAgentBackend>,
    diagnostics: DiagnosticsStore,
    artifacts: ArtifactStore,
    direct_pat: Option<String>,
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

    let generated_artifacts = match backend
        .combine(
            request_dir.path(),
            &combine_request,
            &recorder,
            direct_pat.as_deref(),
        )
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
            let capability = diagnostics.issue(&request_id, recorder.into_events());
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
            return response;
        }
    };

    recorder.record(
        "combine.artifact_retention.attempted",
        Level::Info,
        "attempting to retain generated artifact bytes for authenticated previews and downloads",
    );
    let artifact_list = match artifacts.insert(&request_id, generated_artifacts) {
        Ok(artifacts) => {
            recorder.record(
                "combine.artifact_retention.succeeded",
                Level::Info,
                format!(
                    "retained {} generated artifacts for five minutes",
                    artifacts.len()
                ),
            );
            artifacts
        }
        Err(error) => {
            recorder.record(
                "combine.artifact_retention.failed",
                Level::Error,
                format!("attempted to retain generated artifacts; failed: {error}"),
            );
            let capability = diagnostics.issue(&request_id, recorder.into_events());
            let mut response = error_response_with_details(
                ErrorCode::CombineFailed,
                "failed to retain generated artifacts",
                [
                    format!("Artifact retention error: {error}"),
                    format!("Diagnostics capability: {capability}"),
                ],
            );
            insert_capability_header(&mut response, &capability);
            return response;
        }
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
