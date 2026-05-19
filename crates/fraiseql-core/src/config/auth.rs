//! Authentication configuration.

use serde::{Deserialize, Serialize};

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable authentication.
    pub enabled: bool,

    /// Authentication provider.
    pub provider: AuthProvider,

    /// JWT secret (for jwt provider).
    pub jwt_secret: Option<String>,

    /// JWT algorithm (default: HS256).
    pub jwt_algorithm: String,

    /// Auth0/Clerk domain.
    pub domain: Option<String>,

    /// Auth0/Clerk audience.
    pub audience: Option<String>,

    /// Auth0/Clerk client ID.
    pub client_id: Option<String>,

    /// Header name for auth token.
    pub header_name: String,

    /// Token prefix (e.g., "Bearer ").
    pub token_prefix: String,

    /// Paths to exclude from authentication.
    pub exclude_paths: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AuthProvider::None,
            jwt_secret: None,
            jwt_algorithm: "HS256".to_string(),
            domain: None,
            audience: None,
            client_id: None,
            header_name: "Authorization".to_string(),
            token_prefix: "Bearer ".to_string(),
            exclude_paths: vec!["/health".to_string()],
        }
    }
}

/// Authentication provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AuthProvider {
    /// No authentication.
    #[default]
    None,
    /// Simple JWT authentication.
    Jwt,
    /// Auth0 authentication.
    Auth0,
    /// Clerk authentication.
    Clerk,
    /// Custom webhook-based authentication.
    Webhook,
}
