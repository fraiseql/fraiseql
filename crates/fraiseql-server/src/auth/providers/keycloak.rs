// Keycloak OAuth provider implementation
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Keycloak OAuth provider wrapper
///
/// Handles Keycloak-specific OAuth flows and role mapping.
/// Supports both realm roles and client roles.
#[derive(Debug)]
pub struct KeycloakOAuth {
    oidc:        OidcProvider,
    realm:       String,
    client_name: String,
}

/// Keycloak token claims structure (partial)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeycloakTokenClaims {
    pub sub:                String,
    pub preferred_username: Option<String>,
    pub email:              Option<String>,
    pub name:               Option<String>,
    pub given_name:         Option<String>,
    pub family_name:        Option<String>,
    pub realm_access:       Option<RealmAccess>,
    pub resource_access:    Option<serde_json::Value>,
}

/// Keycloak realm access structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmAccess {
    pub roles: Vec<String>,
}

/// Keycloak client role structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRoles {
    pub roles: Vec<String>,
}

impl KeycloakOAuth {
    /// Create a new Keycloak OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Keycloak client ID (configured in Keycloak realm)
    /// * `client_secret` - Keycloak client secret
    /// * `keycloak_url` - Base Keycloak URL (e.g., "https://keycloak.example.com")
    /// * `realm` - Keycloak realm name (e.g., "master", "fraiseql")
    /// * `redirect_uri` - Redirect URI after authentication
    pub async fn new(
        client_id: String,
        client_secret: String,
        keycloak_url: String,
        realm: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let issuer_url = format!("{}/realms/{}", keycloak_url.trim_end_matches('/'), realm);

        let oidc =
            OidcProvider::new("keycloak", &issuer_url, &client_id, &client_secret, &redirect_uri)
                .await?;

        Ok(Self {
            oidc,
            realm,
            client_name: client_id,
        })
    }

    /// Extract realm roles from JWT claims
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    pub fn extract_realm_roles(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("realm_access")
            .and_then(|ra| ra.get("roles"))
            .and_then(|roles| roles.as_array())
            .map(|roles| roles.iter().filter_map(|r| r.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }

    /// Extract client roles from JWT claims
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    /// * `client_name` - Client name to extract roles for
    pub fn extract_client_roles(raw_claims: &serde_json::Value, client_name: &str) -> Vec<String> {
        raw_claims
            .get("resource_access")
            .and_then(|ra| ra.get(client_name))
            .and_then(|client| client.get("roles"))
            .and_then(|roles| roles.as_array())
            .map(|roles| roles.iter().filter_map(|r| r.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }

    /// Map Keycloak roles to FraiseQL role permissions
    ///
    /// Maps Keycloak roles to FraiseQL role names.
    /// Supports both realm roles and client roles.
    ///
    /// # Arguments
    /// * `keycloak_roles` - List of Keycloak role names
    pub fn map_keycloak_roles_to_fraiseql(keycloak_roles: Vec<String>) -> Vec<String> {
        keycloak_roles
            .into_iter()
            .filter_map(|role| {
                let role_lower = role.to_lowercase();

                match role_lower.as_str() {
                    // Direct Keycloak role names
                    "admin" | "fraiseql-admin" | "administrators" => Some("admin".to_string()),
                    "operator" | "fraiseql-operator" | "operators" => Some("operator".to_string()),
                    "viewer" | "fraiseql-viewer" | "viewers" | "user" => Some("viewer".to_string()),
                    // Realm roles
                    "realm-admin" => Some("admin".to_string()),
                    "realm-operator" => Some("operator".to_string()),
                    // Client roles
                    "client-admin" => Some("admin".to_string()),
                    "client-operator" => Some("operator".to_string()),
                    "client-viewer" => Some("viewer".to_string()),
                    _ => None,
                }
            })
            .collect()
    }

    /// Get all roles (realm + client) from token
    pub fn extract_all_roles(&self, raw_claims: &serde_json::Value) -> Vec<String> {
        let mut roles = Vec::new();

        // Add realm roles
        roles.extend(Self::extract_realm_roles(raw_claims));

        // Add client roles
        roles.extend(Self::extract_client_roles(raw_claims, &self.client_name));

        // Remove duplicates
        roles.sort();
        roles.dedup();

        roles
    }
}

#[async_trait]
impl OAuthProvider for KeycloakOAuth {
    fn name(&self) -> &str {
        "keycloak"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Decode token to extract roles (without full validation, since we got it from OIDC)
        // In a real scenario, you might want to fully validate and parse the JWT here
        let realm_roles = Self::extract_realm_roles(&user_info.raw_claims);
        let client_roles = Self::extract_client_roles(&user_info.raw_claims, &self.client_name);

        // Store roles in raw claims for later mapping
        user_info.raw_claims["keycloak_realm_roles"] = json!(realm_roles);
        user_info.raw_claims["keycloak_client_roles"] = json!(client_roles);
        user_info.raw_claims["keycloak_realm"] = json!(&self.realm);

        // Extract org_id from JWT custom claims if present
        if let Some(org_id_val) = user_info.raw_claims.get("org_id") {
            if let Some(org_id_str) = org_id_val.as_str() {
                user_info.raw_claims["org_id"] = json!(org_id_str);
            }
        } else {
            // Fallback: use realm as org_id if not explicitly set
            user_info.raw_claims["org_id"] = json!(&self.realm);
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
    fn test_extract_realm_roles() {
        let claims = json!({
            "realm_access": {
                "roles": ["admin", "user", "operator"]
            }
        });

        let roles = KeycloakOAuth::extract_realm_roles(&claims);
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_extract_client_roles() {
        let claims = json!({
            "resource_access": {
                "fraiseql": {
                    "roles": ["client-admin", "client-user"]
                }
            }
        });

        let roles = KeycloakOAuth::extract_client_roles(&claims, "fraiseql");
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"client-admin".to_string()));
    }

    #[test]
    fn test_map_keycloak_roles_to_fraiseql() {
        let roles = vec![
            "admin".to_string(),
            "fraiseql-operator".to_string(),
            "user".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = KeycloakOAuth::map_keycloak_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_realm_roles_missing() {
        let claims = json!({});
        let roles = KeycloakOAuth::extract_realm_roles(&claims);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_map_roles_case_insensitive() {
        let roles = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];
        let fraiseql_roles = KeycloakOAuth::map_keycloak_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
    }
}
