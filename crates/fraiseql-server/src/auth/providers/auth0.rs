// Auth0 OAuth provider implementation
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Auth0 OAuth provider wrapper
///
/// Handles Auth0-specific OAuth flows and role mapping.
/// Supports both Auth0 rules and custom claim mapping.
#[derive(Debug)]
pub struct Auth0OAuth {
    oidc:   OidcProvider,
    domain: String,
}

/// Auth0 user information
#[derive(Debug, Clone, Deserialize)]
pub struct Auth0User {
    pub sub:            String,
    pub email:          String,
    pub email_verified: Option<bool>,
    pub name:           Option<String>,
    pub picture:        Option<String>,
    pub locale:         Option<String>,
    pub nickname:       Option<String>,
}

/// Auth0 roles claim
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Auth0Roles {
    pub roles: Option<Vec<String>>,
}

impl Auth0OAuth {
    /// Create a new Auth0 OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Auth0 application client ID
    /// * `client_secret` - Auth0 application client secret
    /// * `auth0_domain` - Auth0 tenant domain (e.g., "example.auth0.com")
    /// * `redirect_uri` - Redirect URI after authentication (e.g., "http://localhost:8000/auth/callback")
    pub async fn new(
        client_id: String,
        client_secret: String,
        auth0_domain: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let issuer_url = format!("https://{}", auth0_domain);

        let oidc =
            OidcProvider::new("auth0", &issuer_url, &client_id, &client_secret, &redirect_uri)
                .await?;

        Ok(Self {
            oidc,
            domain: auth0_domain,
        })
    }

    /// Extract roles from Auth0 custom claims
    ///
    /// Auth0 supports custom claim namespaces to avoid claim collisions.
    /// This extracts roles from the standard Auth0 roles claim or custom namespace.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from Auth0 token
    pub fn extract_roles(raw_claims: &serde_json::Value) -> Vec<String> {
        // Try standard Auth0 roles claim first
        if let Some(roles_val) = raw_claims.get("https://fraiseql.dev/roles") {
            if let Ok(roles) = serde_json::from_value::<Vec<String>>(roles_val.clone()) {
                return roles;
            }
        }

        // Fallback: check for roles array
        if let Some(roles_array) = raw_claims.get("roles") {
            if let Ok(roles) = serde_json::from_value::<Vec<String>>(roles_array.clone()) {
                return roles;
            }
        }

        Vec::new()
    }

    /// Map Auth0 roles to FraiseQL role permissions
    ///
    /// Maps Auth0 role names to FraiseQL role names.
    /// Supports flexible role naming conventions.
    ///
    /// # Arguments
    /// * `auth0_roles` - List of Auth0 role names
    pub fn map_auth0_roles_to_fraiseql(auth0_roles: Vec<String>) -> Vec<String> {
        auth0_roles
            .into_iter()
            .filter_map(|role| {
                let role_lower = role.to_lowercase();

                match role_lower.as_str() {
                    // Direct role matches
                    "admin" | "fraiseql-admin" | "administrators" | "fraiseql_admin" => {
                        Some("admin".to_string())
                    },
                    "operator" | "fraiseql-operator" | "operators" | "fraiseql_operator" => {
                        Some("operator".to_string())
                    },
                    "viewer" | "fraiseql-viewer" | "viewers" | "fraiseql_viewer" | "user"
                    | "fraiseql-user" => Some("viewer".to_string()),
                    // Common patterns
                    "admin_user" => Some("admin".to_string()),
                    "operator_user" => Some("operator".to_string()),
                    "viewer_user" | "read_only" => Some("viewer".to_string()),
                    _ => None,
                }
            })
            .collect()
    }

    /// Extract organization ID from Auth0 claims
    ///
    /// Auth0 supports org_id in custom claims or extracted from domain.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims
    /// * `email` - User email as fallback
    pub fn extract_org_id(raw_claims: &serde_json::Value, email: &str) -> Option<String> {
        // Check for explicit org_id claim
        if let Some(org_id_val) = raw_claims.get("org_id") {
            if let Some(org_id_str) = org_id_val.as_str() {
                return Some(org_id_str.to_string());
            }
        }

        // Fallback: extract from email domain
        email
            .split('@')
            .nth(1)
            .and_then(|domain| domain.split('.').next())
            .map(|domain_part| domain_part.to_string())
    }
}

#[async_trait]
impl OAuthProvider for Auth0OAuth {
    fn name(&self) -> &'static str {
        "auth0"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Extract Auth0-specific claims
        let roles = Self::extract_roles(&user_info.raw_claims);
        user_info.raw_claims["auth0_roles"] = json!(roles);

        // Extract organization ID
        if let Some(org_id) = Self::extract_org_id(&user_info.raw_claims, &user_info.email) {
            user_info.raw_claims["org_id"] = json!(&org_id);
        }

        // Store Auth0 domain for reference
        user_info.raw_claims["auth0_domain"] = json!(&self.domain);

        // Add email verification status
        if let Some(email_verified) = user_info.raw_claims.get("email_verified") {
            user_info.raw_claims["auth0_email_verified"] = email_verified.clone();
        }

        Ok(user_info)
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        self.oidc.refresh_token(refresh_token).await
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        self.oidc.revoke_token(token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_roles_from_custom_namespace() {
        let claims = json!({
            "https://fraiseql.dev/roles": ["admin", "operator", "viewer"]
        });

        let roles = Auth0OAuth::extract_roles(&claims);
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert!(roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_roles_fallback() {
        let claims = json!({
            "roles": ["admin", "user"]
        });

        let roles = Auth0OAuth::extract_roles(&claims);
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_extract_roles_missing() {
        let claims = json!({});
        let roles = Auth0OAuth::extract_roles(&claims);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_map_auth0_roles_to_fraiseql() {
        let roles = vec![
            "admin".to_string(),
            "fraiseql-operator".to_string(),
            "viewer".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_auth0_roles_underscore_separator() {
        let roles = vec![
            "fraiseql_admin".to_string(),
            "fraiseql_operator".to_string(),
            "fraiseql_viewer".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_auth0_roles_case_insensitive() {
        let roles = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
    }

    #[test]
    fn test_map_auth0_roles_common_patterns() {
        let roles = vec![
            "admin_user".to_string(),
            "operator_user".to_string(),
            "viewer_user".to_string(),
            "read_only".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 4);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_claim() {
        let claims = json!({
            "org_id": "example-corp"
        });

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@company.com");
        assert_eq!(org_id, Some("example-corp".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_email_domain() {
        let claims = json!({});

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("example".to_string()));
    }

    #[test]
    fn test_extract_org_id_missing() {
        let claims = json!({});

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@localhost");
        assert_eq!(org_id, Some("localhost".to_string()));
    }

    #[test]
    fn test_extract_org_id_claim_takes_precedence() {
        let claims = json!({
            "org_id": "explicit-org"
        });

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@other.com");
        assert_eq!(org_id, Some("explicit-org".to_string()));
    }
}
