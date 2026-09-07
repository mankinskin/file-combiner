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
mod template_contract;

pub use artifact::GeneratedArtifact;
pub use execution::{FakeCombineBackend, RigCombineBackend};

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
    /// The agent finished without the write tool reporting a created file.
    NoFileCreated,
    /// The tool-reported file failed containment, type, or size validation.
    InvalidOutput(String),
}

impl fmt::Display for CombineAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CombineAgentError::InvalidTemplate(message) => {
                write!(f, "template contract is invalid: {message}")
            }
            CombineAgentError::Execution(message) => write!(f, "agent execution failed: {message}"),
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

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Runs the combinator agent in a fresh request-scoped directory and returns
/// the server-validated artifact manifest. Production requests use
/// [`RigCombineBackend`]; tests inject a deterministic stand-in.
pub trait CombineAgentBackend: Send + Sync {
    fn combine<'a>(
        &'a self,
        dir: &'a Path,
        request: &'a CombineRequest,
        diagnostics: &'a DiagnosticsRecorder<'a>,
        direct_pat: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<GeneratedArtifact>, CombineAgentError>>;
}
