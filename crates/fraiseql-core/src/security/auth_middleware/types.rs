//! Authentication types: `AuthenticatedUser`, `AuthRequest`, `TokenClaims`.

use std::{collections::HashMap, fmt};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::security::errors::{Result, SecurityError};

/// Authenticated user information extracted from JWT claims
///
/// Contains the essential user information needed for authorization checks
/// and audit logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// User ID (from 'sub' claim in JWT)
    pub user_id: String,

    /// Scopes/permissions (from 'scope' claim if present)
    pub scopes: Vec<String>,

    /// When the token expires
    pub expires_at: DateTime<Utc>,

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
            self.user_id,
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
