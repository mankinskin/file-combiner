//! Credential checks for the combine endpoints: shared bearer/probe tokens,
//! per-request group credentials, and direct-PAT fallback.
use axum::http::{header, HeaderMap};
use axum::response::Response;
use base64::Engine;

use crate::contract::ErrorCode;

use super::handler::error_response;
use super::{AppState, PROBE_HEADER};

/// Result of authorization check. Either a rejection response or successful
/// authorization with optional direct PAT.
#[derive(Debug)]
pub(super) enum AuthorizationResult {
    /// Authorization failed; return this response to the client
    Unauthorized(Response),
    /// Authorization succeeded with standard bearer token (no direct PAT)
    Authorized,
    /// Authorization succeeded with a direct PAT for request-scoped usage
    AuthorizedWithDirectPat(String),
}

/// Checks the bearer credential before any file content is processed.
/// Supports three schemes:
/// - `Bearer <token>`: `COMBINE_BEARER_TOKEN` always authorizes; `COMBINE_PROBE_TOKEN` only
///   authorizes when the internal probe header is also present.
/// - `Bearer <direct-pat>`: A bearer value with a recognized GitHub token
///   prefix is authorized for request-scoped direct-PAT use.
/// - `Group <base64(group_name:group_password)>`: validates against configured group credentials.
///
/// Returns an AuthorizationResult indicating the outcome.
pub(super) fn authorize(state: &AppState, headers: &HeaderMap) -> AuthorizationResult {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let Some(auth_header) = auth_header else {
        return unauthorized("missing credentials");
    };

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        return authorize_bearer(state, headers, token);
    }

    if let Some(credentials_b64) = auth_header.strip_prefix("Group ") {
        if let Some(result) = authorize_group(state, credentials_b64) {
            return result;
        }
    }

    unauthorized("invalid credentials")
}

fn unauthorized(message: &str) -> AuthorizationResult {
    AuthorizationResult::Unauthorized(error_response(ErrorCode::Unauthorized, message))
}

/// Handles the `Bearer <token>` scheme: shared bearer token, probe token
/// (requires the probe header), or a plausible direct GitHub PAT.
fn authorize_bearer(state: &AppState, headers: &HeaderMap, token: &str) -> AuthorizationResult {
    if token == state.bearer_token.as_ref() {
        return AuthorizationResult::Authorized;
    }

    if token == state.probe_token.as_ref() {
        return if headers.contains_key(PROBE_HEADER) {
            AuthorizationResult::Authorized
        } else {
            unauthorized("probe token requires probe header")
        };
    }

    if is_plausible_direct_pat(token) {
        return AuthorizationResult::AuthorizedWithDirectPat(token.to_owned());
    }

    unauthorized(
        "invalid credentials: enter the configured shared access key or a valid Copilot personal access token",
    )
}

/// Handles the `Group <base64(name:password)>` scheme. Returns `None` for
/// any decode/parse/config failure so the caller falls back to the shared
/// "invalid credentials" response, matching the prior unified behavior.
fn authorize_group(state: &AppState, credentials_b64: &str) -> Option<AuthorizationResult> {
    let credentials_bytes = base64::engine::general_purpose::STANDARD
        .decode(credentials_b64)
        .ok()?;
    let credentials_str = String::from_utf8(credentials_bytes).ok()?;
    let colon_pos = credentials_str.find(':')?;
    let (provided_group_name, provided_group_password) = credentials_str.split_at(colon_pos);
    let provided_group_password = &provided_group_password[1..];

    let group_config = state.group.as_ref()?;
    let matches = constant_time_eq(
        provided_group_name.as_bytes(),
        group_config.group_name.as_bytes(),
    ) && constant_time_eq(
        provided_group_password.as_bytes(),
        group_config.group_password.as_bytes(),
    );

    matches.then_some(AuthorizationResult::Authorized)
}

fn is_plausible_direct_pat(token: &str) -> bool {
    ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"]
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() > prefix.len())
}

/// Constant-time comparison to prevent timing attacks.
/// Returns true if both slices have the same bytes, false otherwise.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}
