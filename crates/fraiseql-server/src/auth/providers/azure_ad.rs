// Azure AD OAuth provider implementation
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Azure AD OAuth provider wrapper
///
/// Handles Azure Active Directory (Entra) OAuth flows and app role mapping.
/// Supports both app roles and directory roles.
#[derive(Debug)]
pub struct AzureADOAuth {
    oidc: OidcProvider,
    tenant: String,
}

/// Azure AD user information
#[derive(Debug, Clone, Deserialize)]
pub struct AzureADUser {
    pub oid: String,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    #[serde(rename = "jobTitle")]
    pub job_title: Option<String>,
    pub department: Option<String>,
}

impl AzureADOAuth {
    /// Create a new Azure AD OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Azure AD application (client) ID
    /// * `client_secret` - Azure AD client secret
    /// * `tenant` - Azure AD tenant ID or domain (e.g., "contoso.onmicrosoft.com" or "12345678-1234-1234-1234-123456789012")
    /// * `redirect_uri` - Redirect URI after authentication
    pub async fn new(
        client_id: String,
        client_secret: String,
        tenant: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let issuer_url = format!("https://login.microsoftonline.com/{}/v2.0", tenant);

        let oidc = OidcProvider::new(
            "azure_ad",
            &issuer_url,
            &client_id,
            &client_secret,
            &redirect_uri,
        )
        .await?;

        Ok(Self { oidc, tenant })
    }

    /// Extract app roles from JWT claims
    ///
    /// Azure AD stores app roles assigned to the user in the `roles` claim.
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    pub fn extract_app_roles(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("roles")
            .and_then(|roles| roles.as_array())
            .map(|roles| {
                roles
                    .iter()
                    .filter_map(|r| r.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract groups from JWT claims
    ///
    /// Azure AD can include group membership in the `groups` claim if configured.
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    pub fn extract_groups(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("groups")
            .and_then(|groups| groups.as_array())
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(|g| g.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract group IDs from JWT claims (object IDs)
    ///
    /// Azure AD group object IDs for use with Microsoft Graph API
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    pub fn extract_group_ids(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("_claim_sources")
            .and_then(|sources| sources.get("src1"))
            .and_then(|src| src.get("groups"))
            .and_then(|groups| groups.as_array())
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(|g| g.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Map Azure AD app roles to FraiseQL roles
    ///
    /// Azure AD app roles are configured in the application manifest.
    /// Common role naming: fraiseql.admin, fraiseql.operator, fraiseql.viewer
    ///
    /// # Arguments
    /// * `azure_roles` - List of Azure AD app roles
    pub fn map_azure_roles_to_fraiseql(azure_roles: Vec<String>) -> Vec<String> {
        azure_roles
            .into_iter()
            .filter_map(|role| {
                let role_lower = role.to_lowercase();

                // Support various Azure role naming conventions
                if role_lower.contains("admin") {
                    Some("admin".to_string())
                } else if role_lower.contains("operator") {
                    Some("operator".to_string())
                } else if role_lower.contains("viewer") || role_lower.contains("reader") {
                    Some("viewer".to_string())
                } else if role == "fraiseql.admin" || role == "fraiseql_admin" {
                    Some("admin".to_string())
                } else if role == "fraiseql.operator" || role == "fraiseql_operator" {
                    Some("operator".to_string())
                } else if role == "fraiseql.viewer" || role == "fraiseql_viewer" {
                    Some("viewer".to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Extract user's email or principal name
    ///
    /// Azure AD typically provides either email or preferred_username
    pub fn get_user_identifier(raw_claims: &serde_json::Value) -> Option<String> {
        raw_claims
            .get("preferred_username")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                raw_claims
                    .get("email")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string())
            })
    }

    /// Get user's display name
    pub fn get_user_display_name(raw_claims: &serde_json::Value) -> Option<String> {
        raw_claims
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
    }
}

#[async_trait]
impl OAuthProvider for AzureADOAuth {
    fn name(&self) -> &str {
        "azure_ad"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Extract Azure AD specific claims
        let app_roles = Self::extract_app_roles(&user_info.raw_claims);
        let groups = Self::extract_groups(&user_info.raw_claims);
        let group_ids = Self::extract_group_ids(&user_info.raw_claims);

        // Store extracted data for role mapping
        user_info.raw_claims["azure_app_roles"] = json!(app_roles);
        user_info.raw_claims["azure_groups"] = json!(groups);
        user_info.raw_claims["azure_group_ids"] = json!(group_ids);
        user_info.raw_claims["azure_tenant"] = json!(&self.tenant);

        // Extract user identifier (UPN or email)
        if let Some(identifier) = Self::get_user_identifier(&user_info.raw_claims) {
            user_info.raw_claims["azure_user_identifier"] = json!(identifier);
        }

        // Extract display name
        if let Some(display_name) = Self::get_user_display_name(&user_info.raw_claims) {
            user_info.raw_claims["azure_display_name"] = json!(display_name);
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
    fn test_extract_app_roles() {
        let claims = json!({
            "roles": ["fraiseql.admin", "fraiseql.operator"]
        });

        let roles = AzureADOAuth::extract_app_roles(&claims);
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"fraiseql.admin".to_string()));
    }

    #[test]
    fn test_extract_groups() {
        let claims = json!({
            "groups": [
                "00000000-0000-0000-0000-000000000001",
                "00000000-0000-0000-0000-000000000002"
            ]
        });

        let groups = AzureADOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_map_azure_roles_to_fraiseql() {
        let roles = vec![
            "fraiseql.admin".to_string(),
            "fraiseql.operator".to_string(),
            "fraiseql.viewer".to_string(),
            "other.role".to_string(),
        ];

        let fraiseql_roles = AzureADOAuth::map_azure_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_azure_roles_underscore_format() {
        let roles = vec![
            "fraiseql_admin".to_string(),
            "fraiseql_operator".to_string(),
        ];

        let fraiseql_roles = AzureADOAuth::map_azure_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 2);
    }

    #[test]
    fn test_map_azure_roles_case_insensitive() {
        let roles = vec![
            "FRAISEQL.ADMIN".to_string(),
            "FraiseQL.Operator".to_string(),
        ];

        let fraiseql_roles = AzureADOAuth::map_azure_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 2);
    }

    #[test]
    fn test_get_user_identifier_upn() {
        let claims = json!({
            "preferred_username": "user@contoso.com"
        });

        let identifier = AzureADOAuth::get_user_identifier(&claims);
        assert_eq!(identifier, Some("user@contoso.com".to_string()));
    }

    #[test]
    fn test_get_user_identifier_email_fallback() {
        let claims = json!({
            "email": "user@contoso.com"
        });

        let identifier = AzureADOAuth::get_user_identifier(&claims);
        assert_eq!(identifier, Some("user@contoso.com".to_string()));
    }

    #[test]
    fn test_extract_app_roles_missing() {
        let claims = json!({});
        let roles = AzureADOAuth::extract_app_roles(&claims);
        assert!(roles.is_empty());
    }
}
