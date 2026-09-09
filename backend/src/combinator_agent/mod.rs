//! The thin `combinator-agent` adapter: builds the combine prompt, exposes
//! the one write-capable tool the model may use, and validates whatever file
//! that tool reports having created. No other tool, path, or fallback is
//! available to the model.
use std::fmt;
use std::path::Path;
use std::pin::Pin;

use crate::diagnostics::DiagnosticsRecorder;

mod artifact;
mod execution;
pub mod instructions;
pub mod latent;
mod template_contract;

pub use artifact::GeneratedArtifact;
pub use execution::{FakeCombineBackend, MockCombineBackend, RigCombineBackend};
pub use latent::{AnalysisKind, LatentRepresentation};

/// Maximum artifacts a template may request in one combine operation.
pub const MAX_ARTIFACTS: usize = 8;
/// Maximum bytes the write tool accepts for one artifact body.
pub const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
/// Maximum bytes accepted across all artifacts in one combine operation.
pub const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// The two uploaded files the combinator agent is asked to combine.
pub struct CombineRequest {
    pub first_name: String,
    pub first_contents: String,
    pub second_name: String,
    pub second_contents: String,
}

#[derive(Debug)]
pub enum CombineAgentError {
    /// The uploaded template did not declare a valid artifact contract.
    InvalidTemplate(String),
    /// The model/tool run itself failed.
    Execution(String),
    /// The configured Copilot credentials are missing or invalid for the model.
    ModelUnauthenticated(String),
    /// The agent finished without the write tool reporting a created file.
    NoFileCreated,
    /// The tool-reported file failed containment, type, or size validation.
    InvalidOutput(String),
}

pub(crate) fn classify_execution_error(message: impl Into<String>) -> CombineAgentError {
    let message = message.into();
    if message.contains("GITHUB_COPILOT_TOKEN or COPILOT_API_KEY must be set") {
        CombineAgentError::ModelUnauthenticated(
            "Copilot authentication is not configured on the server. Set GITHUB_COPILOT_TOKEN or COPILOT_API_KEY.".to_owned(),
        )
    } else {
        CombineAgentError::Execution(message)
    }
}

impl fmt::Display for CombineAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CombineAgentError::InvalidTemplate(message) => {
                write!(f, "template contract is invalid: {message}")
            }
            CombineAgentError::Execution(message) => write!(f, "agent execution failed: {message}"),
            CombineAgentError::ModelUnauthenticated(message) => {
                write!(f, "{message}")
            }
            CombineAgentError::NoFileCreated => {
                write!(f, "agent did not create a combined file through its tool")
            }
            CombineAgentError::InvalidOutput(message) => {
                write!(f, "combined file failed validation: {message}")
            }
        }
    }
}

impl std::error::Error for CombineAgentError {}

#[cfg(test)]
mod tests {
    use super::{classify_execution_error, CombineAgentError};

    #[test]
    fn classify_execution_error_detects_missing_copilot_credentials() {
        let error = classify_execution_error(
            "agent execution failed: GITHUB_COPILOT_TOKEN or COPILOT_API_KEY must be set",
        );

        assert!(matches!(error, CombineAgentError::ModelUnauthenticated(_)));
        assert_eq!(
            error.to_string(),
            "Copilot authentication is not configured on the server. Set GITHUB_COPILOT_TOKEN or COPILOT_API_KEY."
        );
    }
}

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Runs the combinator agent in a fresh request-scoped directory and returns
/// the server-validated artifact manifest. Production requests use
/// [`RigCombineBackend`]; tests inject a deterministic stand-in.
pub trait CombineAgentBackend: Send + Sync {
    fn analyze<'a>(
        &'a self,
        kind: AnalysisKind,
        name: &'a str,
        contents: &'a str,
        diagnostics: &'a DiagnosticsRecorder<'a>,
        direct_pat: Option<&'a str>,
    ) -> BoxFuture<'a, Result<LatentRepresentation, CombineAgentError>>;

    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
        diagnostics: &'a DiagnosticsRecorder<'a>,
        direct_pat: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<GeneratedArtifact>, CombineAgentError>>;
}
