//! In-memory storage for generated artifact bytes, keyed by request and
//! artifact id, bounded by a TTL and a maximum stored-artifact count.
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::combinator_agent::{CombineAgentError, GeneratedArtifact};

const ARTIFACT_TTL: Duration = Duration::from_secs(300);
const MAX_STORED_ARTIFACTS: usize = 64;

#[derive(Debug, Serialize)]
pub(super) struct ArtifactResponse {
    id: String,
    name: String,
    format: String,
    download_url: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ArtifactListResponse {
    pub(super) artifacts: Vec<ArtifactResponse>,
}

#[derive(Clone)]
pub(super) struct ArtifactStore(Arc<Mutex<HashMap<String, StoredArtifact>>>);

#[derive(Clone)]
struct StoredArtifact {
    content: Vec<u8>,
    expires_at: Instant,
}

impl ArtifactStore {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(super) fn insert(
        &self,
        request_id: &str,
        artifacts: Vec<GeneratedArtifact>,
    ) -> Result<Vec<ArtifactResponse>, CombineAgentError> {
        let mut stored = self.0.lock().unwrap();
        let now = Instant::now();
        stored.retain(|_, artifact| artifact.expires_at > now);
        if stored.len() + artifacts.len() > MAX_STORED_ARTIFACTS {
            return Err(CombineAgentError::InvalidOutput(
                "artifact store capacity is exhausted".to_owned(),
            ));
        }

        let mut response = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            let content = fs::read(&artifact.path)
                .map_err(|error| CombineAgentError::InvalidOutput(error.to_string()))?;
            let key = format!("{request_id}/{}", artifact.id);
            stored.insert(
                key,
                StoredArtifact {
                    content,
                    expires_at: now + ARTIFACT_TTL,
                },
            );
            response.push(ArtifactResponse {
                download_url: format!("/api/artifacts/{request_id}/{}", artifact.id),
                id: artifact.id,
                name: artifact.name,
                format: artifact.format,
            });
        }
        Ok(response)
    }

    pub(super) fn get(&self, request_id: &str, artifact_id: &str) -> Option<Vec<u8>> {
        let mut stored = self.0.lock().unwrap();
        let now = Instant::now();
        stored.retain(|_, artifact| artifact.expires_at > now);
        stored
            .get(&format!("{request_id}/{artifact_id}"))
            .map(|artifact| artifact.content.clone())
    }
}
