// Authentication middleware for Axum
use crate::auth::error::{AuthError, Result};
use crate::auth::jwt::Claims;
use crate::auth::session::SessionStore;
use crate::auth::jwt::JwtValidator;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// Authenticated user extracted from JWT token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    /// User ID from token claims
    pub user_id: String,
    /// Full JWT claims
    pub claims: Claims,
}

impl AuthenticatedUser {
    /// Get a custom claim from the JWT
    pub fn get_custom_claim(&self, key: &str) -> Option<&serde_json::Value> {
        self.claims.get_custom(key)
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        if let Some(serde_json::Value::String(user_role)) = self.claims.get_custom("role") {
            user_role == role
        } else if let Some(serde_json::Value::Array(roles)) = self.claims.get_custom("roles") {
            roles.iter().any(|r| {
                if let serde_json::Value::String(r_str) = r {
                    r_str == role
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
}

/// Authentication middleware configuration
pub struct AuthMiddleware {
    validator: Arc<JwtValidator>,
    _session_store: Arc<dyn SessionStore>,
    public_key: Vec<u8>,
    _optional: bool,
}

impl AuthMiddleware {
    /// Create a new authentication middleware
    ///
    /// # Arguments
    /// * `validator` - JWT validator
    /// * `session_store` - Session storage backend
    /// * `public_key` - Public key for JWT signature verification
    /// * `optional` - If true, missing auth is not an error
    pub fn new(
        validator: Arc<JwtValidator>,
        session_store: Arc<dyn SessionStore>,
        public_key: Vec<u8>,
        optional: bool,
    ) -> Self {
        Self {
            validator,
            _session_store: session_store,
            public_key,
            _optional: optional,
        }
    }

    /// Validate a Bearer token and extract claims
    pub async fn validate_token(&self, token: &str) -> Result<Claims> {
        self.validator.validate(token, &self.public_key)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            AuthError::TokenExpired => (
                StatusCode::UNAUTHORIZED,
                "token_expired",
                "Authentication token has expired",
            ),
            AuthError::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "invalid_signature",
                "Token signature is invalid",
            ),
            AuthError::InvalidToken { ref reason } => (
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                reason.as_str(),
            ),
            AuthError::TokenNotFound => (
                StatusCode::UNAUTHORIZED,
                "token_not_found",
                "Authentication token not found",
            ),
            AuthError::SessionRevoked => (
                StatusCode::UNAUTHORIZED,
                "session_revoked",
                "Session has been revoked",
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth_error",
                "An authentication error occurred",
            ),
        };

        let body = serde_json::json!({
            "errors": [{
                "message": message,
                "extensions": {
                    "code": error
                }
            }]
        });

        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authenticated_user_clone() {
        use crate::auth::Claims;
        use std::collections::HashMap;

        let claims = Claims {
            sub: "user123".to_string(),
            iat: 1000,
            exp: 2000,
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            extra: HashMap::new(),
        };

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        let _cloned = user.clone();
        assert_eq!(user.user_id, "user123");
    }

    #[test]
    fn test_has_role_single_string() {
        use crate::auth::Claims;
        use std::collections::HashMap;

        let mut claims = Claims {
            sub: "user123".to_string(),
            iat: 1000,
            exp: 2000,
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims
            .extra
            .insert("role".to_string(), serde_json::json!("admin"));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert!(user.has_role("admin"));
        assert!(!user.has_role("user"));
    }

    #[test]
    fn test_has_role_array() {
        use crate::auth::Claims;
        use std::collections::HashMap;

        let mut claims = Claims {
            sub: "user123".to_string(),
            iat: 1000,
            exp: 2000,
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims.extra.insert(
            "roles".to_string(),
            serde_json::json!(["admin", "user", "editor"]),
        );

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert!(user.has_role("admin"));
        assert!(user.has_role("user"));
        assert!(user.has_role("editor"));
        assert!(!user.has_role("moderator"));
    }

    #[test]
    fn test_get_custom_claim() {
        use crate::auth::Claims;
        use std::collections::HashMap;

        let mut claims = Claims {
            sub: "user123".to_string(),
            iat: 1000,
            exp: 2000,
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims
            .extra
            .insert("org_id".to_string(), serde_json::json!("org_456"));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert_eq!(
            user.get_custom_claim("org_id"),
            Some(&serde_json::json!("org_456"))
        );
        assert_eq!(user.get_custom_claim("nonexistent"), None);
    }
}
