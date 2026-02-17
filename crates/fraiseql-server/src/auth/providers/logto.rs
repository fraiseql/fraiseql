// Logto OAuth provider implementation
use async_trait::async_trait;
use serde_json::json;

use crate::auth::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Logto OAuth provider wrapper
///
/// Handles Logto-specific OAuth flows and role/organization mapping.
/// Supports both Logto Cloud and self-hosted deployments.
///
/// # Examples
///
/// ```ignore
/// let provider = LogtoOAuth::new(
///     "client_id".to_string(),
///     "client_secret".to_string(),
///     "https://your-tenant.logto.app".to_string(),
///     "http://localhost:8000/auth/callback".to_string(),
/// ).await?;
/// ```
#[derive(Debug)]
pub struct LogtoOAuth {
    oidc:     OidcProvider,
    endpoint: String,
}

impl LogtoOAuth {
    /// Create a new Logto OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Logto application ID
    /// * `client_secret` - Logto application secret
    /// * `logto_endpoint` - Logto base endpoint (e.g., <https://your-tenant.logto.app>) The `/oidc`
    ///   path is automatically appended
    /// * `redirect_uri` - Redirect URI after authentication
    ///
    /// # Errors
    /// Returns error if OIDC discovery fails
    pub async fn new(
        client_id: String,
        client_secret: String,
        logto_endpoint: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let endpoint = logto_endpoint.clone();

        // Automatically append /oidc to the endpoint
        let issuer_url = format!("{}/oidc", logto_endpoint.trim_end_matches('/'));

        let oidc =
            OidcProvider::new("logto", &issuer_url, &client_id, &client_secret, &redirect_uri)
                .await?;

        Ok(Self { oidc, endpoint })
    }

    /// Extract roles from JWT claims
    ///
    /// Looks for `roles` claim which is an array of role strings.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    fn extract_roles(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("roles")
            .and_then(|roles| roles.as_array())
            .map(|roles| roles.iter().filter_map(|r| r.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }

    /// Extract organizations from JWT claims
    ///
    /// Looks for `organizations` claim which is an array of organization IDs.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    fn extract_organizations(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("organizations")
            .and_then(|orgs| orgs.as_array())
            .map(|orgs| orgs.iter().filter_map(|o| o.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }

    /// Extract organization-scoped roles from JWT claims
    ///
    /// Looks for `organization_roles` claim which maps org IDs to role arrays.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    fn extract_organization_roles(raw_claims: &serde_json::Value) -> Vec<String> {
        let mut org_roles = Vec::new();

        if let Some(org_roles_obj) = raw_claims.get("organization_roles") {
            if let Some(obj) = org_roles_obj.as_object() {
                for (_, roles_val) in obj {
                    if let Some(roles_arr) = roles_val.as_array() {
                        for role in roles_arr {
                            if let Some(role_str) = role.as_str() {
                                org_roles.push(role_str.to_string());
                            }
                        }
                    }
                }
            }
        }

        org_roles
    }

    /// Extract current organization ID from JWT claims
    ///
    /// Looks for `organization_id` claim which represents the current org context.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    fn extract_organization_id(raw_claims: &serde_json::Value) -> Option<String> {
        raw_claims
            .get("organization_id")
            .and_then(|org_id| org_id.as_str())
            .map(|s| s.to_string())
    }

    /// Map Logto roles to FraiseQL role permissions
    ///
    /// Maps Logto role names to FraiseQL role names.
    /// Supports organization patterns and substring matching for unknown roles.
    ///
    /// # Arguments
    /// * `logto_roles` - List of Logto role names
    pub fn map_logto_roles_to_fraiseql(logto_roles: Vec<String>) -> Vec<String> {
        logto_roles
            .into_iter()
            .filter_map(|role| {
                let role_lower = role.to_lowercase();

                // Match direct role names
                match role_lower.as_str() {
                    "admin" | "logto-admin" | "administrator" => Some("admin".to_string()),
                    "operator" | "logto-operator" => Some("operator".to_string()),
                    "viewer" | "logto-viewer" | "user" | "member" => Some("viewer".to_string()),
                    _ => {
                        // Match organization patterns (e.g., "organization:admin")
                        if role_lower.contains("organization") {
                            if role_lower.contains("admin") {
                                Some("admin".to_string())
                            } else if role_lower.contains("operator") {
                                Some("operator".to_string())
                            } else if role_lower.contains("member") || role_lower.contains("user") {
                                Some("viewer".to_string())
                            } else {
                                None
                            }
                        } else {
                            // Substring matching for unknown patterns
                            if role_lower.contains("admin") {
                                Some("admin".to_string())
                            } else if role_lower.contains("operator") {
                                Some("operator".to_string())
                            } else if role_lower.contains("viewer") {
                                Some("viewer".to_string())
                            } else {
                                None
                            }
                        }
                    },
                }
            })
            .collect()
    }
}

#[async_trait]
impl OAuthProvider for LogtoOAuth {
    fn name(&self) -> &'static str {
        "logto"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Extract Logto-specific claims
        let roles = Self::extract_roles(&user_info.raw_claims);
        let organizations = Self::extract_organizations(&user_info.raw_claims);
        let org_roles = Self::extract_organization_roles(&user_info.raw_claims);
        let mapped_roles = Self::map_logto_roles_to_fraiseql(roles.clone());

        user_info.raw_claims["logto_roles"] = json!(roles);
        user_info.raw_claims["logto_organizations"] = json!(organizations.clone());
        user_info.raw_claims["logto_organization_roles"] = json!(org_roles);
        user_info.raw_claims["fraiseql_roles"] = json!(mapped_roles);
        user_info.raw_claims["logto_endpoint"] = json!(&self.endpoint);

        // Extract org_id (current org or first in list)
        if let Some(org_id) = Self::extract_organization_id(&user_info.raw_claims) {
            user_info.raw_claims["org_id"] = json!(org_id);
        } else if !organizations.is_empty() {
            user_info.raw_claims["org_id"] = json!(&organizations[0]);
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
    fn test_extract_roles_from_claim() {
        let claims = json!({
            "roles": ["admin", "operator", "viewer"]
        });

        let roles = LogtoOAuth::extract_roles(&claims);
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_roles_missing() {
        let claims = json!({});
        let roles = LogtoOAuth::extract_roles(&claims);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_extract_organizations() {
        let claims = json!({
            "organizations": ["org-1", "org-2", "org-3"]
        });

        let orgs = LogtoOAuth::extract_organizations(&claims);
        assert_eq!(orgs.len(), 3);
        assert!(orgs.contains(&"org-1".to_string()));
    }

    #[test]
    fn test_extract_organizations_missing() {
        let claims = json!({});
        let orgs = LogtoOAuth::extract_organizations(&claims);
        assert!(orgs.is_empty());
    }

    #[test]
    fn test_extract_organization_roles() {
        let claims = json!({
            "organization_roles": {
                "org-1": ["admin"],
                "org-2": ["member", "operator"]
            }
        });

        let org_roles = LogtoOAuth::extract_organization_roles(&claims);
        assert_eq!(org_roles.len(), 3);
        assert!(org_roles.contains(&"admin".to_string()));
        assert!(org_roles.contains(&"member".to_string()));
        assert!(org_roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_organization_roles_missing() {
        let claims = json!({});
        let org_roles = LogtoOAuth::extract_organization_roles(&claims);
        assert!(org_roles.is_empty());
    }

    #[test]
    fn test_extract_organization_id() {
        let claims = json!({
            "organization_id": "current-org"
        });

        let org_id = LogtoOAuth::extract_organization_id(&claims);
        assert_eq!(org_id, Some("current-org".to_string()));
    }

    #[test]
    fn test_extract_organization_id_missing() {
        let claims = json!({});
        let org_id = LogtoOAuth::extract_organization_id(&claims);
        assert!(org_id.is_none());
    }

    #[test]
    fn test_map_logto_roles_to_fraiseql() {
        let roles = vec![
            "admin".to_string(),
            "logto-operator".to_string(),
            "user".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_logto_roles_case_insensitive() {
        let roles = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_logto_roles_organization_pattern() {
        let roles = vec![
            "organization:admin".to_string(),
            "organization:member".to_string(),
            "organization:operator".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_map_logto_roles_substring_matching() {
        let roles = vec![
            "my_custom_admin_role".to_string(),
            "operator_special".to_string(),
            "viewer_guest".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_org_id_fallback_to_first_org() {
        let claims = json!({
            "organizations": ["org-1", "org-2"]
        });

        let org_id = LogtoOAuth::extract_organization_id(&claims);
        assert!(org_id.is_none()); // Should be None because organization_id is missing

        // Simulating the fallback logic from user_info()
        let orgs = LogtoOAuth::extract_organizations(&claims);
        let fallback_org = if orgs.is_empty() {
            None
        } else {
            Some(orgs[0].clone())
        };

        assert_eq!(fallback_org, Some("org-1".to_string()));
    }

    #[test]
    fn test_extract_all_claims() {
        let claims = json!({
            "roles": ["admin", "operator"],
            "organizations": ["org-1", "org-2"],
            "organization_id": "org-1",
            "organization_roles": {
                "org-1": ["admin"]
            }
        });

        let roles = LogtoOAuth::extract_roles(&claims);
        let orgs = LogtoOAuth::extract_organizations(&claims);
        let org_id = LogtoOAuth::extract_organization_id(&claims);
        let org_roles = LogtoOAuth::extract_organization_roles(&claims);
        let mapped_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles.clone());

        assert_eq!(roles.len(), 2);
        assert_eq!(orgs.len(), 2);
        assert_eq!(org_id, Some("org-1".to_string()));
        assert_eq!(org_roles.len(), 1);
        assert_eq!(mapped_roles.len(), 2);
    }
}
