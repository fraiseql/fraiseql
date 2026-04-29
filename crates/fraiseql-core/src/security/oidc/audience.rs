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

    // NOTE: `email`, `email_verified`, and `name` are intentionally NOT
    // named fields. Some providers (e.g. Hanko) emit `email` as a nested
    // object `{"address": "…", "is_verified": true}` rather than a flat
    // string.  Keeping them as named `Option<String>` fields would fail
    // serde deserialization for those providers.  By omitting them, both
    // flat strings and nested objects fall through to the `extra` catch-all
    // (`HashMap<String, serde_json::Value>`) and are available via
    // `expose_claims` and enrichment parameter binding.

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_audience_none() {
        let aud = Audience::None;
        assert!(!aud.contains("test"));
        assert!(aud.to_vec().is_empty());
    }

    #[test]
    fn test_audience_single() {
        let aud = Audience::Single("my-api".to_string());
        assert!(aud.contains("my-api"));
        assert!(!aud.contains("other"));
        assert_eq!(aud.to_vec(), vec!["my-api"]);
    }

    #[test]
    fn test_audience_multiple() {
        let aud = Audience::Multiple(vec!["api1".to_string(), "api2".to_string()]);
        assert!(aud.contains("api1"));
        assert!(aud.contains("api2"));
        assert!(!aud.contains("api3"));
        assert_eq!(aud.to_vec(), vec!["api1", "api2"]);
    }

    #[test]
    fn test_extra_claims_captures_namespaced_claim() {
        let claims_json = r#"{
            "sub": "user123",
            "exp": 1735689600,
            "https://myapp.com/role": "admin",
            "tenant_id": "acme-corp"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.extra.get("https://myapp.com/role"), Some(&serde_json::json!("admin")));
        assert_eq!(claims.extra.get("tenant_id"), Some(&serde_json::json!("acme-corp")));
    }

    #[test]
    fn test_email_and_name_land_in_extra() {
        // email, email_verified, and name are NOT named fields — they fall
        // through to `extra` so that providers emitting objects (Hanko) and
        // providers emitting strings (Auth0) both deserialize correctly.
        let claims_json = r#"{
            "sub": "user123",
            "exp": 1735689600,
            "email": "user@example.com",
            "name": "Alice"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.extra.get("email"), Some(&serde_json::json!("user@example.com")));
        assert_eq!(claims.extra.get("name"), Some(&serde_json::json!("Alice")));
    }

    #[test]
    fn test_email_as_nested_object_deserializes() {
        // Hanko emits email as {"address": "…", "is_primary": true, "is_verified": true}
        let claims_json = r#"{
            "sub": "user123",
            "exp": 1735689600,
            "email": {"address": "user@example.com", "is_primary": true, "is_verified": true}
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        let email = claims.extra.get("email").unwrap();
        assert_eq!(email["address"], "user@example.com");
        assert_eq!(email["is_verified"], true);
    }

    #[test]
    fn test_extra_claims_empty_when_no_unknowns() {
        let claims_json = r#"{"sub": "user123", "exp": 1735689600}"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert!(claims.extra.is_empty());
    }

    #[test]
    fn test_jwt_claims_deserialization() {
        let claims_json = r#"{
            "sub": "user123",
            "iss": "https://issuer.example.com",
            "aud": "my-api",
            "exp": 1735689600,
            "iat": 1735686000,
            "scope": "read write",
            "email": "user@example.com"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.sub, Some("user123".to_string()));
        assert_eq!(claims.iss, Some("https://issuer.example.com".to_string()));
        assert!(claims.aud.contains("my-api"));
        assert_eq!(claims.exp, Some(1_735_689_600));
        assert_eq!(claims.scope, Some("read write".to_string()));
    }

    #[test]
    fn test_jwt_claims_array_audience() {
        let claims_json = r#"{
            "sub": "user123",
            "aud": ["api1", "api2"],
            "exp": 1735689600
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert!(claims.aud.contains("api1"));
        assert!(claims.aud.contains("api2"));
    }
}
