//! Request-scoped, capability-gated Agent Builder/Rig diagnostics. Only
//! allowlisted, server-authored, redacted structured events are ever
//! stored; no raw stderr, prompts, or secrets are captured.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::Serialize;
use tokio::time::Instant;

/// How long a diagnostics capability remains valid for repeat reads.
pub const CAPABILITY_TTL: Duration = Duration::from_secs(15 * 60);
/// Maximum structured events retained per request.
const MAX_EVENTS: usize = 50;
/// Maximum total message bytes retained per request.
const MAX_MESSAGE_BYTES: usize = 16 * 1024;

/// Severity of a diagnostic event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// One allowlisted, redacted, structured diagnostic record correlated to a
/// single combine request.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticEvent {
    pub request_id: String,
    pub kind: &'static str,
    pub level: Level,
    pub timestamp_ms: u128,
    pub message: String,
}

impl DiagnosticEvent {
    pub fn new(
        request_id: &str,
        kind: &'static str,
        level: Level,
        message: impl Into<String>,
    ) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            request_id: request_id.to_owned(),
            kind,
            level,
            timestamp_ms,
            message: redact(&message.into()),
        }
    }
}

/// Redacts any known secret substrings from a message before it is ever
/// stored, as defense in depth on top of only ever constructing
/// fixed-shape, count-only messages.
fn redact(message: &str) -> String {
    let mut redacted = message.to_owned();
    for secret in secret_env_values() {
        if !secret.is_empty() {
            redacted = redacted.replace(&secret, "[redacted]");
        }
    }
    redacted
}

fn secret_env_values() -> Vec<String> {
    ["COMBINE_BEARER_TOKEN", "COMBINE_PROBE_TOKEN", "GITHUB_COPILOT_TOKEN", "COPILOT_API_KEY"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .collect()
}

/// The bounded, possibly-truncated event set returned for one request.
#[derive(Debug, Clone, Serialize)]
pub struct RequestDiagnostics {
    pub events: Vec<DiagnosticEvent>,
    pub truncated: bool,
}

/// Bounds and finalizes a request's raw event list before it is stored.
fn bound_events(request_id: &str, mut events: Vec<DiagnosticEvent>) -> RequestDiagnostics {
    let mut truncated = false;
    if events.len() > MAX_EVENTS {
        events.truncate(MAX_EVENTS);
        truncated = true;
    }

    let mut bounded = Vec::with_capacity(events.len());
    let mut total_bytes = 0usize;
    for event in events {
        let size = event.message.len();
        if total_bytes + size > MAX_MESSAGE_BYTES {
            truncated = true;
            break;
        }
        total_bytes += size;
        bounded.push(event);
    }

    if truncated {
        bounded.push(DiagnosticEvent::new(
            request_id,
            "diagnostics.truncated",
            Level::Warn,
            "additional records were dropped to stay within retention limits",
        ));
    }

    RequestDiagnostics {
        events: bounded,
        truncated,
    }
}

fn generate_capability() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Entry {
    diagnostics: RequestDiagnostics,
    expires_at: Instant,
}

/// In-memory, TTL-bounded store mapping opaque diagnostics capabilities to
/// one request's bounded event set. Expired entries are swept lazily.
#[derive(Clone)]
pub struct DiagnosticsStore {
    entries: std::sync::Arc<Mutex<HashMap<String, Entry>>>,
}

impl DiagnosticsStore {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Bounds and stores `events`, returning the opaque capability a caller
    /// must present (with the shared bearer) to read them back.
    pub fn issue(&self, request_id: &str, events: Vec<DiagnosticEvent>) -> String {
        let diagnostics = bound_events(request_id, events);
        let capability = generate_capability();
        let now = Instant::now();

        let mut guard = self.entries.lock().unwrap();
        guard.retain(|_, entry| entry.expires_at > now);
        guard.insert(
            capability.clone(),
            Entry {
                diagnostics,
                expires_at: now + CAPABILITY_TTL,
            },
        );
        capability
    }

    /// Returns the stored diagnostics for `capability` if it exists and has
    /// not expired; supports repeat reads within the TTL.
    pub fn retrieve(&self, capability: &str) -> Option<RequestDiagnostics> {
        let mut guard = self.entries.lock().unwrap();
        let now = Instant::now();
        match guard.get(capability) {
            Some(entry) if entry.expires_at > now => Some(entry.diagnostics.clone()),
            Some(_) => {
                guard.remove(capability);
                None
            }
            None => None,
        }
    }
}

impl Default for DiagnosticsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_configured_secret_values() {
        std::env::set_var("COMBINE_BEARER_TOKEN", "super-secret-bearer");
        let message = redact("failed while using super-secret-bearer to authorize");
        assert!(!message.contains("super-secret-bearer"));
        assert!(message.contains("[redacted]"));
        std::env::remove_var("COMBINE_BEARER_TOKEN");
    }

    #[test]
    fn bound_events_marks_truncation_past_the_event_limit() {
        let events: Vec<_> = (0..MAX_EVENTS + 5)
            .map(|i| DiagnosticEvent::new("req-1", "combine.received", Level::Info, format!("event {i}")))
            .collect();

        let diagnostics = bound_events("req-1", events);

        assert!(diagnostics.truncated);
        assert!(diagnostics.events.len() <= MAX_EVENTS + 1);
        assert_eq!(diagnostics.events.last().unwrap().kind, "diagnostics.truncated");
    }

    #[tokio::test(start_paused = true)]
    async fn issued_capability_supports_repeat_reads_then_expires() {
        let store = DiagnosticsStore::new();
        let events = vec![DiagnosticEvent::new("req-1", "combine.completed", Level::Info, "ok")];
        let capability = store.issue("req-1", events);

        assert!(store.retrieve(&capability).is_some());
        assert!(store.retrieve(&capability).is_some(), "repeat reads must succeed within the TTL");

        tokio::time::advance(CAPABILITY_TTL + Duration::from_secs(1)).await;

        assert!(store.retrieve(&capability).is_none(), "capability must expire after the TTL");
    }

    #[test]
    fn unknown_capability_returns_none() {
        let store = DiagnosticsStore::new();
        assert!(store.retrieve("does-not-exist").is_none());
    }
}
