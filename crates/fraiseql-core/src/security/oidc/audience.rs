//! Audience validation types for OIDC token claims.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// JWT Claims
// ============================================================================

/// Standard JWT claims for validation.
#[derive(Debug, Clone, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID)
    pub sub: Option<String>,

    /// Issuer
    pub iss: Option<String>,

    /// Audience (can be string or array)
    #[serde(default)]
    pub aud: Audience,

    /// Expiration time (Unix timestamp)
    pub exp: Option<i64>,

    /// Issued at (Unix timestamp)
    pub iat: Option<i64>,

    /// Not before (Unix timestamp)
    pub nbf: Option<i64>,

    /// JWT ID — unique identifier for this token.
    ///
    /// Used by the replay cache to detect reuse of a stolen token.
    pub jti: Option<String>,

    /// Scope (space-separated string, common in Auth0/Okta)
    pub scope: Option<String>,

    /// Scopes (array, common in some providers)
    pub scp: Option<Vec<String>>,

    /// Permissions (array, common in Auth0)
    pub permissions: Option<Vec<String>>,

    /// Email claim
    pub email: Option<String>,

    /// Email verified
    pub email_verified: Option<bool>,

    /// Name claim
    pub name: Option<String>,

    /// Arbitrary extra claims not captured by named fields above.
    ///
    /// Captures custom OIDC claims such as `"email"`, `"tenant_id"`, or
    /// namespaced claims like `"https://myapp.com/role"` that are not part of
    /// the standard JWT claim set.  Used by `GET /auth/me` to reflect a
    /// configurable subset of the token's claims to the frontend.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Audience can be a single string or array of strings.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Audience {
    /// No audience specified.
    #[default]
    None,
    /// Single audience string.
    Single(String),
    /// Multiple audiences as an array.
    Multiple(Vec<String>),
}

impl Audience {
    /// Check if the audience contains a specific value.
    pub fn contains(&self, value: &str) -> bool {
        match self {
            Self::None => false,
            Self::Single(s) => s == value,
            Self::Multiple(v) => v.iter().any(|s| s == value),
        }
    }

    /// Get all audience values as a vector.
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::Single(s) => vec![s.clone()],
            Self::Multiple(v) => v.clone(),
        }
    }
}
