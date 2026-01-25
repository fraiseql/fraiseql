// Google OAuth provider implementation
use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Google OAuth provider wrapper
///
/// Handles Google-specific OAuth flows and Workspace group mapping to FraiseQL roles.
#[derive(Debug)]
pub struct GoogleOAuth {
    oidc: OidcProvider,
}

/// Google user information
#[derive(Debug, Clone, Deserialize)]
pub struct GoogleUser {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub locale: Option<String>,
}

/// Google Workspace group
#[derive(Debug, Clone, Deserialize)]
pub struct GoogleWorkspaceGroup {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

impl GoogleOAuth {
    /// Create a new Google OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Google OAuth client ID (from Google Cloud Console)
    /// * `client_secret` - Google OAuth client secret
    /// * `redirect_uri` - Redirect URI after authentication (e.g., "http://localhost:8000/auth/callback")
    pub async fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let oidc = OidcProvider::new(
            "google",
            "https://accounts.google.com",
            &client_id,
            &client_secret,
            &redirect_uri,
        )
        .await?;

        Ok(Self { oidc })
    }

    /// Map Google Workspace groups to FraiseQL roles
    ///
    /// Maps group emails/names to role names based on naming conventions.
    /// Example: "fraiseql-admins@company.com" -> "admin"
    ///
    /// # Arguments
    /// * `groups` - List of group email addresses
    pub fn map_groups_to_roles(groups: Vec<String>) -> Vec<String> {
        groups
            .into_iter()
            .filter_map(|group| {
                let group_lower = group.to_lowercase();

                // Check common admin group names
                if group_lower.contains("fraiseql-admin")
                    || group_lower.contains("fraiseql-admins")
                    || group_lower.contains("-admin@")
                    || group_lower.contains("-admins@")
                {
                    return Some("admin".to_string());
                }

                // Check operator group names
                if group_lower.contains("fraiseql-operator")
                    || group_lower.contains("fraiseql-operators")
                    || group_lower.contains("-operator@")
                    || group_lower.contains("-operators@")
                {
                    return Some("operator".to_string());
                }

                // Check viewer group names
                if group_lower.contains("fraiseql-viewer")
                    || group_lower.contains("fraiseql-viewers")
                    || group_lower.contains("-viewer@")
                    || group_lower.contains("-viewers@")
                {
                    return Some("viewer".to_string());
                }

                None
            })
            .collect()
    }

    /// Check if user belongs to a specific group
    ///
    /// Simple email-based check without Directory API (for basic use cases)
    pub fn extract_roles_from_domain(email: &str) -> Vec<String> {
        // Default roles based on email domain
        // This is a fallback when Directory API is not available
        if email.ends_with("@company.com") {
            // Company employees get operator role by default
            vec!["operator".to_string()]
        } else {
            vec!["viewer".to_string()]
        }
    }
}

#[async_trait]
impl OAuthProvider for GoogleOAuth {
    fn name(&self) -> &str {
        "google"
    }

    fn authorization_url(&self, state: &str) -> String {
        // Add additional scopes for Workspace directory access if needed
        // Note: This requires configuration of the authorization URL with scopes
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        // Get user info from OIDC
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Extract domain-based roles as fallback
        let default_roles = Self::extract_roles_from_domain(&user_info.email);
        user_info.raw_claims["google_default_roles"] = serde_json::json!(default_roles);

        // Extract org_id from email domain
        let org_id = user_info
            .email
            .split('@')
            .nth(1)
            .and_then(|domain| domain.split('.').next())
            .map(|domain_part| domain_part.to_string());

        if let Some(org_id) = org_id {
            user_info.raw_claims["org_id"] = serde_json::json!(&org_id);
        }

        // Note: To get Workspace groups, you would need to:
        // 1. Request additional scopes: https://www.googleapis.com/auth/admin.directory.group.readonly
        // 2. Use Directory API: GET https://www.googleapis.com/admin/directory/v1/groups?userKey={email}
        // This requires admin consent and service account setup, so it's not included in basic setup
        //
        // For now, we store the email for later group lookup
        user_info.raw_claims["google_email"] = serde_json::json!(&user_info.email);
        user_info.raw_claims["google_workspace_available"] =
            serde_json::json!("Configure Directory API scopes for group sync");

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
    fn test_map_google_workspace_groups_to_roles() {
        let groups = vec![
            "fraiseql-admins@company.com".to_string(),
            "fraiseql-operators@company.com".to_string(),
            "other-group@company.com".to_string(),
            "fraiseql-viewer@company.com".to_string(),
        ];

        let roles = GoogleOAuth::map_groups_to_roles(groups);

        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert!(roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_groups_case_insensitive() {
        let groups = vec![
            "FRAISEQL-ADMINS@COMPANY.COM".to_string(),
            "FraiseQL-Operators@Company.Com".to_string(),
        ];

        let roles = GoogleOAuth::map_groups_to_roles(groups);

        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_roles_from_domain_company() {
        let roles = GoogleOAuth::extract_roles_from_domain("user@company.com");
        assert_eq!(roles, vec!["operator".to_string()]);
    }

    #[test]
    fn test_extract_roles_from_domain_external() {
        let roles = GoogleOAuth::extract_roles_from_domain("user@external.com");
        assert_eq!(roles, vec!["viewer".to_string()]);
    }

    #[test]
    fn test_map_groups_empty() {
        let roles = GoogleOAuth::map_groups_to_roles(vec![]);
        assert!(roles.is_empty());
    }
}
