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
/// - `Bearer <direct-pat>`: Any Bearer token that doesn't match the known tokens is treated
///   as a direct Copilot PAT and authorized for request-scoped use.
/// - `Group <base64(group_name:group_password)>`: validates against configured group credentials.
///
/// Returns an AuthorizationResult indicating the outcome.
pub(super) fn authorize(state: &AppState, headers: &HeaderMap) -> AuthorizationResult {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let Some(auth_header) = auth_header else {
        return AuthorizationResult::Unauthorized(error_response(
            ErrorCode::Unauthorized,
            "missing credentials",
        ));
    };

    // Check for Bearer scheme
    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        if token == state.bearer_token.as_ref() {
            return AuthorizationResult::Authorized;
        }

        if token == state.probe_token.as_ref() {
            if headers.contains_key(PROBE_HEADER) {
                return AuthorizationResult::Authorized;
            } else {
                // Probe token without the required probe header is rejected
                return AuthorizationResult::Unauthorized(error_response(
                    ErrorCode::Unauthorized,
                    "probe token requires probe header",
                ));
            }
        }

        // If Bearer token doesn't match known tokens, treat it as a direct PAT
        // and authorize it for request-scoped use
        if !token.is_empty() {
            return AuthorizationResult::AuthorizedWithDirectPat(token.to_owned());
        }
    }
    // Check for Group scheme
    else if let Some(credentials_b64) = auth_header.strip_prefix("Group ") {
        // Decode base64 credentials
        let credentials_bytes =
            match base64::engine::general_purpose::STANDARD.decode(credentials_b64) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return AuthorizationResult::Unauthorized(error_response(
                        ErrorCode::Unauthorized,
                        "invalid credentials",
                    ));
                }
            };

        let credentials_str = match String::from_utf8(credentials_bytes) {
            Ok(s) => s,
            Err(_) => {
                return AuthorizationResult::Unauthorized(error_response(
                    ErrorCode::Unauthorized,
                    "invalid credentials",
                ));
            }
        };

        // Parse group_name:group_password
        if let Some(colon_pos) = credentials_str.find(':') {
            let (provided_group_name, provided_group_password) =
                credentials_str.split_at(colon_pos);
            let provided_group_password = &provided_group_password[1..];

            if let Some(group_config) = &state.group {
                // Use constant-time comparison to avoid timing attacks
                if constant_time_eq(
                    provided_group_name.as_bytes(),
                    group_config.group_name.as_bytes(),
                ) && constant_time_eq(
                    provided_group_password.as_bytes(),
                    group_config.group_password.as_bytes(),
                ) {
                    return AuthorizationResult::Authorized;
                }
            }
        }
    }

    AuthorizationResult::Unauthorized(error_response(
        ErrorCode::Unauthorized,
        "invalid credentials",
    ))
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
