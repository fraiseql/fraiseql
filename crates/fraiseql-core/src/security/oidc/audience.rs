//! Audience validation types for OIDC token claims.

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
