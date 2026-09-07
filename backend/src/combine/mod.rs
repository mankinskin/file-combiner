//! The authenticated `/api/combine` endpoint: bounded, in-memory multipart handling.
mod artifact_store;
mod auth;
mod handler;
mod pipeline;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::Semaphore;

use crate::combinator_agent::CombineAgentBackend;
use crate::config::GroupConfig;
use crate::contract::MAX_CONCURRENT_COMBINES;
use crate::diagnostics::DiagnosticsStore;

use artifact_store::ArtifactStore;
use handler::{combine, diagnostics, download_artifact};

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
    group: Option<GroupConfig>,
    combine_slots: Arc<Semaphore>,
    backend: Arc<dyn CombineAgentBackend>,
    diagnostics: DiagnosticsStore,
    artifacts: ArtifactStore,
}

impl AppState {
    pub fn new(
        bearer_token: String,
        probe_token: String,
        backend: Arc<dyn CombineAgentBackend>,
    ) -> Self {
        AppState::with_group(bearer_token, probe_token, None, backend)
    }

    pub fn with_group(
        bearer_token: String,
        probe_token: String,
        group: Option<GroupConfig>,
        backend: Arc<dyn CombineAgentBackend>,
    ) -> Self {
        AppState {
            bearer_token: bearer_token.into(),
            probe_token: probe_token.into(),
            group,
            combine_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_COMBINES)),
            backend,
            diagnostics: DiagnosticsStore::new(),
            artifacts: ArtifactStore::new(),
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
        .route(
            "/api/artifacts/:request_id/:artifact_id",
            get(download_artifact),
        )
        .route("/api/diagnostics/:capability", get(diagnostics))
        .with_state(state)
}
