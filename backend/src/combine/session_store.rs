use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::combinator_agent::LatentRepresentation;

const SESSION_TTL: Duration = Duration::from_secs(900);
const MAX_SESSIONS: usize = 64;

#[derive(Clone)]
pub(super) struct AnalysisSessionStore(Arc<Mutex<HashMap<String, AnalysisSession>>>);

struct AnalysisSession {
    template: Option<LatentRepresentation>,
    profile: Option<LatentRepresentation>,
    artifacts: Vec<SessionArtifact>,
    expires_at: Instant,
}

#[derive(Clone)]
pub(super) struct SessionArtifact {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) format: String,
    pub(super) content: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub(super) struct AnalysisResponse {
    pub(super) session_id: String,
    pub(super) complete: bool,
    pub(super) representations: Vec<LatentRepresentation>,
    pub(super) artifacts: Vec<SessionArtifactResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct SessionArtifactResponse {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) format: String,
    pub(super) download_url: String,
}

impl AnalysisSessionStore {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(super) fn upsert(
        &self,
        requested_id: Option<&str>,
        representation: LatentRepresentation,
    ) -> Result<AnalysisResponse, &'static str> {
        let mut sessions = self.0.lock().map_err(|_| "session store unavailable")?;
        let now = Instant::now();
        sessions.retain(|_, session| session.expires_at > now);
        let session_id = requested_id
            .filter(|id| sessions.contains_key(*id))
            .map(str::to_owned)
            .unwrap_or_else(new_session_id);
        if !sessions.contains_key(&session_id) && sessions.len() >= MAX_SESSIONS {
            return Err("session capacity is exhausted");
        }

        let session = sessions
            .entry(session_id.clone())
            .or_insert_with(|| AnalysisSession {
                template: None,
                profile: None,
                artifacts: Vec::new(),
                expires_at: now + SESSION_TTL,
            });
        session.expires_at = now + SESSION_TTL;
        match representation.source_kind {
            crate::combinator_agent::latent::SourceKind::Template => {
                session.template = Some(representation)
            }
            crate::combinator_agent::latent::SourceKind::PersonProfile => {
                session.profile = Some(representation)
            }
            crate::combinator_agent::latent::SourceKind::Combined => {
                return Err("combined representations cannot be uploaded")
            }
        }

        if session.template.is_some() && session.profile.is_some() && session.artifacts.is_empty() {
            let combined = crate::combinator_agent::latent::combine_representations(
                session.template.as_ref().expect("checked above"),
                session.profile.as_ref().expect("checked above"),
            );
            session.artifacts = generate_minimum_artifacts(&combined);
        }

        let mut representations = Vec::new();
        if let Some(template) = &session.template {
            representations.push(template.clone());
        }
        if let Some(profile) = &session.profile {
            representations.push(profile.clone());
        }
        let complete = session.template.is_some() && session.profile.is_some();
        let artifacts = session
            .artifacts
            .iter()
            .map(|artifact| SessionArtifactResponse {
                id: artifact.id.clone(),
                name: artifact.name.clone(),
                format: artifact.format.clone(),
                download_url: format!("/api/sessions/{session_id}/artifacts/{}", artifact.id),
            })
            .collect();
        Ok(AnalysisResponse {
            session_id,
            complete,
            representations,
            artifacts,
        })
    }

    pub(super) fn contains(&self, session_id: &str) -> bool {
        let mut sessions = match self.0.lock() {
            Ok(sessions) => sessions,
            Err(_) => return false,
        };
        let now = Instant::now();
        sessions.retain(|_, session| session.expires_at > now);
        sessions.contains_key(session_id)
    }

    pub(super) fn get_artifact(
        &self,
        session_id: &str,
        artifact_id: &str,
    ) -> Option<SessionArtifact> {
        let mut sessions = self.0.lock().ok()?;
        let now = Instant::now();
        sessions.retain(|_, session| session.expires_at > now);
        sessions
            .get(session_id)?
            .artifacts
            .iter()
            .find(|artifact| artifact.id == artifact_id)
            .cloned()
    }
}

fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("session-{nanos:x}")
}

fn generate_minimum_artifacts(combined: &LatentRepresentation) -> Vec<SessionArtifact> {
    let overview = format!(
        "# Closer overview\n\nTemplate: {}\n\nCustomer situation:\n{}\n\nGoals:\n{}\n\nLikely questions:\n{}\n",
        combined.title,
        combined.customer.situation.join("\n- "),
        combined.customer.goals.join("\n- "),
        combined.questions.join("\n- "),
    );
    let guide = format!(
        "# Individualized conversation guide\n\n{}\n\n## Conversation stages\n{}\n\n## Objections to explore\n{}\n",
        combined.title,
        combined
            .stages
            .iter()
            .map(|stage| format!("### {}\n{}", stage.name, stage.questions.join("\n- ")))
            .collect::<Vec<_>>()
            .join("\n\n"),
        combined.objections.join("\n- "),
    );
    vec![
        SessionArtifact {
            id: "artifact-0".to_owned(),
            name: "closer-overview".to_owned(),
            format: "markdown".to_owned(),
            content: overview.into_bytes(),
        },
        SessionArtifact {
            id: "artifact-1".to_owned(),
            name: "individualized-conversation-guide".to_owned(),
            format: "markdown".to_owned(),
            content: guide.into_bytes(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinator_agent::latent::{
        AnalysisKind, CustomerContext, LatentRepresentation, SourceKind,
    };

    fn representation(kind: AnalysisKind, title: &str) -> LatentRepresentation {
        LatentRepresentation {
            schema_version: crate::combinator_agent::latent::LATENT_SCHEMA_VERSION,
            source_kind: match kind {
                AnalysisKind::Template => SourceKind::Template,
                AnalysisKind::PersonProfile => SourceKind::PersonProfile,
            },
            title: title.to_owned(),
            domain: Some("sales".to_owned()),
            stages: Vec::new(),
            customer: CustomerContext::default(),
            motivations: Vec::new(),
            questions: Vec::new(),
            objections: Vec::new(),
            tone: Vec::new(),
            source_spans: Vec::new(),
        }
    }

    #[test]
    fn partial_session_becomes_complete_with_minimum_artifacts() {
        let store = AnalysisSessionStore::new();
        let partial = store
            .upsert(None, representation(AnalysisKind::Template, "Template"))
            .unwrap();
        assert!(!partial.complete);
        assert_eq!(partial.representations.len(), 1);

        let complete = store
            .upsert(
                Some(&partial.session_id),
                representation(AnalysisKind::PersonProfile, "Profile"),
            )
            .unwrap();
        assert!(complete.complete);
        assert_eq!(complete.representations.len(), 2);
        assert_eq!(complete.artifacts.len(), 2);
        assert!(store
            .get_artifact(&complete.session_id, "artifact-0")
            .is_some());
    }
}
