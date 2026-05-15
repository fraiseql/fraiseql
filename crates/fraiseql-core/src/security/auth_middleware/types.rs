//! Authentication types: `AuthenticatedUser`, `AuthRequest`, `TokenClaims`.

use std::{collections::HashMap, fmt};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::security::errors::{Result, SecurityError};
use crate::types::UserId;

/// Authenticated user information extracted from JWT claims
///
/// Contains the essential user information needed for authorization checks
/// and audit logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// User ID (from 'sub' claim in JWT)
    pub user_id: UserId,

    /// Scopes/permissions (from 'scope' claim if present)
    pub scopes: Vec<String>,

    /// When the token expires
    pub expires_at: DateTime<Utc>,

    /// Normalised email address extracted from the JWT `email` claim.
    ///
    /// Handles flat strings and nested objects (`{"value": "..."}`,
    /// `{"email": "..."}`).  `None` when the claim is absent or cannot
    /// be normalised to a non-empty string.
    pub email: Option<String>,

    /// Normalised display name extracted from the JWT `name` claim.
    ///
    /// Handles flat strings, nested objects with `formatted` key, and
    /// `given`+`family` concatenation.  `None` when the claim is absent
    /// or cannot be normalised.
    pub display_name: Option<String>,

    /// Arbitrary extra claims from the JWT, forwarded by the OIDC validator.
    ///
    /// Populated from the `#[serde(flatten)] extra` field on `JwtClaims` when
    /// the OIDC validation path is used.  Empty when tokens are validated via
    /// the legacy `AuthMiddleware` signing-key path or API-key authentication.
    ///
    /// Used by `GET /auth/me` to surface a configurable subset of custom OIDC
    /// claims (e.g. `"email"`, `"tenant_id"`, namespaced claims) to the
    /// frontend without requiring client-side script to touch the `HttpOnly` cookie.
    pub extra_claims: HashMap<String, serde_json::Value>,
}

impl fmt::Display for AuthenticatedUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "User({}, expires={})",
            self.user_id.as_str(),
            self.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
}

impl AuthenticatedUser {
    /// Check if the user has a specific scope
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    /// Check if the token has expired (as of now)
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Get time until expiry in seconds
    #[must_use]
    pub fn ttl_secs(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds()
    }
}

/// Request context for authentication validation
///
/// Contains information extracted from an HTTP request.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    /// Raw Authorization header value (e.g., "Bearer eyJhbG...")
    pub authorization_header: Option<String>,
}

impl AuthRequest {
    /// Create a new auth request from an authorization header
    #[must_use]
    pub const fn new(authorization_header: Option<String>) -> Self {
        Self {
            authorization_header,
        }
    }

    /// Extract the bearer token from the Authorization header.
    ///
    /// Expected format: `"Bearer <token>"`
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::AuthRequired`] if the Authorization header is
    /// absent or does not start with `"Bearer "`.
    pub fn extract_bearer_token(&self) -> Result<String> {
        let header = self.authorization_header.as_ref().ok_or(SecurityError::AuthRequired)?;

        if !header.starts_with("Bearer ") {
            return Err(SecurityError::AuthRequired);
        }

        Ok(header[7..].to_string())
    }
}

/// Claims extracted from JWT token
///
/// This is a simplified representation of JWT claims.
/// In production, this would be more comprehensive.
#[derive(Debug, Clone)]
pub struct TokenClaims {
    /// Subject (user ID)
    pub sub: Option<String>,

    /// Expiration timestamp (seconds since epoch)
    pub exp: Option<i64>,

    /// Scope claim (space-separated string)
    pub scope: Option<String>,

    /// Audience claim
    pub aud: Option<Vec<String>>,

    /// Issuer claim
    pub iss: Option<String>,
}

/// JWT claims structure for jsonwebtoken crate deserialization.
///
/// This struct is used internally for decoding and validating JWT tokens
/// when signature verification is enabled.
#[derive(Debug, Deserialize)]
pub(super) struct JwtClaims {
    /// Subject (user ID) - required
    pub(super) sub: Option<String>,

    /// Expiration timestamp (seconds since epoch) - required
    pub(super) exp: Option<i64>,

    /// Issued at timestamp (captured but not used directly)
    #[serde(default)]
    #[allow(dead_code)] // Reason: serde deserialization target, validated by jsonwebtoken
    pub(super) iat: Option<i64>,

    /// Not before timestamp (captured but not used directly)
    #[serde(default)]
    #[allow(dead_code)] // Reason: serde deserialization target, validated by jsonwebtoken
    pub(super) nbf: Option<i64>,

    /// Scope claim (space-separated string)
    #[serde(default)]
    pub(super) scope: Option<String>,

    /// Scopes as array (alternative format used by some providers)
    #[serde(default)]
    pub(super) scp: Option<Vec<String>>,

    /// Permissions (Auth0 RBAC style)
    #[serde(default)]
    pub(super) permissions: Option<Vec<String>>,

    /// Audience claim (validated by jsonwebtoken, captured for logging)
    #[serde(default)]
    #[allow(dead_code)] // Reason: serde deserialization target, validated by jsonwebtoken
    pub(super) aud: Option<serde_json::Value>,

    /// Issuer claim (validated by jsonwebtoken, captured for logging)
    #[serde(default)]
    #[allow(dead_code)] // Reason: serde deserialization target, validated by jsonwebtoken
    pub(super) iss: Option<String>,

    /// Arbitrary extra claims not captured by named fields above.
    ///
    /// Passed through to `AuthenticatedUser.extra_claims` so that custom OIDC
    /// claims are available to handlers such as `GET /auth/me`.
    #[serde(flatten)]
    pub(super) extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Nested claim extraction helpers
// ---------------------------------------------------------------------------

/// Trim a string and return `None` if the result is empty.
fn trim_or_none(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_owned()) }
}

/// Extract a flat string from a potentially nested JWT claim value.
///
/// Handles: plain strings, objects with `value`/`formatted`/`email` keys
/// (falls back to first string value), and arrays (first string element).
/// Returns `None` for null/number/bool/empty values.
pub(crate) fn extract_claim_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => trim_or_none(s),
        serde_json::Value::Object(map) => {
            for key in &["value", "formatted", "email"] {
                if let Some(serde_json::Value::String(s)) = map.get(*key) {
                    if let Some(v) = trim_or_none(s) {
                        return Some(v);
                    }
                }
            }
            map.values().find_map(|v| {
                if let serde_json::Value::String(s) = v {
                    trim_or_none(s)
                } else {
                    None
                }
            })
        },
        serde_json::Value::Array(arr) => arr.iter().find_map(|v| {
            if let serde_json::Value::String(s) = v {
                trim_or_none(s)
            } else {
                None
            }
        }),
        _ => None,
    }
}

/// Extract a display name from a potentially nested JWT `name` claim.
///
/// Tries [`extract_claim_string`] first for strings/arrays, then handles
/// objects with `value`/`formatted` keys and `given`+`family` concatenation.
pub(crate) fn extract_name_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(_) | serde_json::Value::Array(_) => {
            extract_claim_string(value)
        },
        serde_json::Value::Object(map) => {
            for key in &["value", "formatted", "email"] {
                if let Some(serde_json::Value::String(s)) = map.get(*key) {
                    if let Some(v) = trim_or_none(s) {
                        return Some(v);
                    }
                }
            }
            let given = map.get("given").and_then(|v| v.as_str()).and_then(trim_or_none);
            let family = map.get("family").and_then(|v| v.as_str()).and_then(trim_or_none);
            match (given, family) {
                (Some(g), Some(f)) => Some(format!("{g} {f}")),
                (Some(g), None) => Some(g),
                (None, Some(f)) => Some(f),
                (None, None) => None,
            }
        },
        _ => None,
    }
}
